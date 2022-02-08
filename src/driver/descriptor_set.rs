use {
    super::{DescriptorSetLayout, Device, DriverError},
    crate::ptr::Shared,
    archery::SharedPointerKind,
    ash::vk,
    derive_builder::Builder,
    log::{trace, warn},
    std::{ops::Deref, thread::panicking},
};

#[derive(Debug)]
pub struct DescriptorPool<P>
where
    P: SharedPointerKind,
{
    pub info: DescriptorPoolInfo,
    descriptor_pool: vk::DescriptorPool,
    pub device: Shared<Device<P>, P>,
}

impl<P> DescriptorPool<P>
where
    P: SharedPointerKind,
{
    pub fn create(
        device: &Shared<Device<P>, P>,
        info: impl Into<DescriptorPoolInfo>,
    ) -> Result<Self, DriverError> {
        let device = Shared::clone(device);
        let info = info.into();
        let descriptor_pool = unsafe {
            device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::builder()
                    .flags(vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET)
                    .max_sets(info.max_sets)
                    .pool_sizes(
                        &info
                            .pool_sizes
                            .iter()
                            .map(|pool_size| vk::DescriptorPoolSize {
                                ty: pool_size.ty,
                                descriptor_count: pool_size.descriptor_count,
                            })
                            .collect::<Box<[_]>>(),
                    ),
                None,
            )
        }
        .map_err(|_| DriverError::Unsupported)?;

        Ok(Self {
            descriptor_pool,
            device,
            info,
        })
    }

    pub fn allocate_descriptor_set(
        this: &Shared<Self, P>,
        layout: &DescriptorSetLayout<P>,
    ) -> Result<DescriptorSet<P>, DriverError> {
        Ok(Self::allocate_descriptor_sets(this, layout, 1)?.remove(0))
    }

    pub fn allocate_descriptor_sets(
        this: &Shared<Self, P>,
        layout: &DescriptorSetLayout<P>,
        count: u32,
    ) -> Result<Vec<DescriptorSet<P>>, DriverError> {
        use std::slice::from_ref;

        let descriptor_pool = Shared::clone(this);
        let mut create_info = vk::DescriptorSetAllocateInfo::builder()
            .descriptor_pool(this.descriptor_pool)
            .set_layouts(from_ref(layout));
        create_info.descriptor_set_count = count;
        let create_info = create_info.build();

        trace!("allocate_descriptor_sets");

        Ok(unsafe {
            this.device
                .allocate_descriptor_sets(&create_info)
                .map_err(|_| DriverError::Unsupported)?
                .iter()
                .map(|descriptor_set| DescriptorSet {
                    descriptor_pool: Shared::clone(&descriptor_pool),
                    descriptor_set: *descriptor_set,
                })
                .collect()
        })
    }
}

impl<P> Deref for DescriptorPool<P>
where
    P: SharedPointerKind,
{
    type Target = vk::DescriptorPool;

    fn deref(&self) -> &Self::Target {
        &self.descriptor_pool
    }
}

impl<P> Drop for DescriptorPool<P>
where
    P: SharedPointerKind,
{
    fn drop(&mut self) {
        if panicking() {
            return;
        }

        unsafe {
            self.device
                .destroy_descriptor_pool(self.descriptor_pool, None);
        }
    }
}

#[derive(Builder, Clone, Debug, Eq, Hash, PartialEq)]
#[builder(pattern = "owned", derive(Debug))]
pub struct DescriptorPoolInfo {
    pub max_sets: u32,
    pub pool_sizes: Vec<DescriptorPoolSize>,
}

impl DescriptorPoolInfo {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(max_sets: u32) -> DescriptorPoolInfoBuilder {
        DescriptorPoolInfoBuilder::default().max_sets(max_sets)
    }
}

impl From<DescriptorPoolInfoBuilder> for DescriptorPoolInfo {
    fn from(info: DescriptorPoolInfoBuilder) -> Self {
        info.build().unwrap()
    }
}

#[derive(Builder, Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[builder(pattern = "owned")]
pub struct DescriptorPoolSize {
    pub ty: vk::DescriptorType,
    pub descriptor_count: u32,
}

#[derive(Debug)]
pub struct DescriptorSet<P>
where
    P: SharedPointerKind,
{
    descriptor_pool: Shared<DescriptorPool<P>, P>,
    descriptor_set: vk::DescriptorSet,
}

impl<P> Deref for DescriptorSet<P>
where
    P: SharedPointerKind,
{
    type Target = vk::DescriptorSet;

    fn deref(&self) -> &Self::Target {
        &self.descriptor_set
    }
}

impl<P> Drop for DescriptorSet<P>
where
    P: SharedPointerKind,
{
    fn drop(&mut self) {
        use std::slice::from_ref;

        if panicking() {
            return;
        }

        unsafe {
            self.descriptor_pool
                .device
                .free_descriptor_sets(**self.descriptor_pool, from_ref(&self.descriptor_set))
                .unwrap_or_else(|_| warn!("Unable to free descriptor set"))
        }
    }
}