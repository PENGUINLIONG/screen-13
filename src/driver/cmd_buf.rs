use {
    super::{device::Device, DriverError},
    ash::vk,
    log::{error, trace, warn},
    std::{fmt::Debug, ops::Deref, sync::Arc, thread::panicking},
};

/// Represents a Vulkan command buffer to which some work has been submitted.
#[derive(Debug)]
pub struct CommandBuffer {
    cmd_buf: vk::CommandBuffer,
    pub(crate) device: Arc<Device>,
    droppables: Vec<Box<dyn Debug + Send + 'static>>,
    pub(crate) fence: vk::Fence, // Keeps state because everyone wants this
    pub(crate) query_pool: vk::QueryPool,

    /// Information used to create this object.
    pub info: CommandBufferInfo,

    pub(crate) pool: vk::CommandPool,
}

impl CommandBuffer {
    #[profiling::function]
    pub(crate) fn create(
        device: &Arc<Device>,
        info: CommandBufferInfo,
    ) -> Result<Self, DriverError> {
        let device = Arc::clone(device);
        let cmd_pool_info = vk::CommandPoolCreateInfo::builder()
            .flags(
                vk::CommandPoolCreateFlags::TRANSIENT
                    | vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER,
            )
            .queue_family_index(info.queue_family_index);
        let pool = unsafe {
            device
                .create_command_pool(&cmd_pool_info, None)
                .map_err(|err| {
                    warn!("{err}");

                    DriverError::Unsupported
                })?
        };
        let cmd_buf_info = vk::CommandBufferAllocateInfo::builder()
            .command_buffer_count(1)
            .command_pool(pool)
            .level(vk::CommandBufferLevel::PRIMARY);
        let cmd_buf = unsafe {
            device
                .allocate_command_buffers(&cmd_buf_info)
                .map_err(|err| {
                    warn!("{err}");

                    DriverError::Unsupported
                })?
        }[0];
        let fence = Device::create_fence(&device, true)?;
        let query_pool = Device::create_query_pool(&device, vk::QueryType::TIMESTAMP, 2)?;

        Ok(Self {
            cmd_buf,
            device,
            droppables: vec![],
            fence,
            query_pool,
            info,
            pool,
        })
    }

    /// Signals that execution has completed and it is time to drop anything we collected.
    #[profiling::function]
    pub(crate) fn drop_fenced(this: &mut Self) {
        if !this.droppables.is_empty() {
            trace!("dropping {} shared references", this.droppables.len());
        }

        this.droppables.clear();
    }

    /// Returns `true` after the GPU has executed the previous submission to this command buffer.
    ///
    /// See [`Self::wait_until_executed`] to block while checking.
    #[profiling::function]
    pub fn has_executed(&self) -> Result<bool, DriverError> {
        let res = unsafe { self.device.get_fence_status(self.fence) };

        match res {
            Ok(status) => Ok(status),
            Err(err) if err == vk::Result::ERROR_DEVICE_LOST => {
                error!("Device lost");

                Err(DriverError::InvalidData)
            }
            Err(err) => {
                // VK_SUCCESS and VK_NOT_READY handled by get_fence_status in ash
                // VK_ERROR_DEVICE_LOST already handled above, so no idea what happened
                error!("{}", err);

                Err(DriverError::InvalidData)
            }
        }
    }

    /// Drops an item after execution has been completed
    pub(crate) fn push_fenced_drop(this: &mut Self, thing_to_drop: impl Debug + Send + 'static) {
        this.droppables.push(Box::new(thing_to_drop));
    }

    /// Stalls by blocking the current thread until the GPU has executed the previous submission to
    /// this command buffer.
    ///
    /// See [`Self::has_executed`] to check without blocking.
    #[profiling::function]
    pub fn wait_until_executed(&self) -> Result<(), DriverError> {
        Device::wait_for_fence(&self.device, &self.fence)
    }

    /// Get timestamp query results.
    #[profiling::function]
    pub fn get_query_results(&self) -> Result<[u64; 2], DriverError> {
        let mut results = [0; 2];
        unsafe {
            self.device
                .get_query_pool_results(
                    self.query_pool,
                    0,
                    2,
                    &mut results,
                    vk::QueryResultFlags::TYPE_64 | vk::QueryResultFlags::WAIT,
                )
                .map_err(|err| {
                    error!("{}", err);

                    DriverError::InvalidData
                })?;
        }

        Ok(results)
    }
}

impl Deref for CommandBuffer {
    type Target = vk::CommandBuffer;

    fn deref(&self) -> &Self::Target {
        &self.cmd_buf
    }
}

impl Drop for CommandBuffer {
    #[profiling::function]
    fn drop(&mut self) {
        use std::slice::from_ref;

        if panicking() {
            return;
        }

        unsafe {
            if Device::wait_for_fence(&self.device, &self.fence).is_err() {
                return;
            }

            self.device
                .free_command_buffers(self.pool, from_ref(&self.cmd_buf));
            self.device.destroy_command_pool(self.pool, None);
            self.device.destroy_query_pool(self.query_pool, None);
            self.device.destroy_fence(self.fence, None);
        }
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct CommandBufferInfo {
    pub queue_family_index: u32,
}

impl CommandBufferInfo {
    pub fn new(queue_family_index: u32) -> Self {
        Self { queue_family_index }
    }
}
