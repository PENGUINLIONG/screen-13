mod lease;

pub use self::lease::Lease;

use {
    super::{
        driver::{CommandPool, DescriptorPool, Driver, Fence, Image2d, Memory, RenderPass},
        op::Compiler,
        render_passes::{color, draw},
        BlendMode, Compute, ComputeMode, Data, Graphics, GraphicsMode, RenderPassMode, Texture,
        TextureRef,
    },
    crate::math::Extent,
    gfx_hal::{
        buffer::Usage as BufferUsage,
        format::Format,
        image::{Layout, Usage as ImageUsage},
        pool::CommandPool as _,
        pso::{DescriptorRangeDesc, DescriptorType},
        queue::QueueFamilyId,
        MemoryTypeId,
    },
    std::{
        cell::RefCell,
        collections::{HashMap, VecDeque},
        rc::Rc,
    },
};

#[cfg(debug_assertions)]
use gfx_hal::device::Device as _;

const DEFAULT_LRU_THRESHOLD: usize = 8;

fn remove_last_by<T, F: Fn(&T) -> bool>(items: &mut VecDeque<T>, f: F) -> Option<T> {
    // let len = items.len();
    // TODO: This is no longer remove by last!!
    for idx in 0..items.len() {
        if f(&items[idx]) {
            return Some(items.remove(idx).unwrap());
        }
    }

    None
}

pub(super) type PoolRef<T> = Rc<RefCell<VecDeque<T>>>;

#[derive(Eq, Hash, PartialEq)]
struct DescriptorPoolKey {
    desc_ranges: Vec<(DescriptorType, usize)>,
}

pub struct Drain<'a>(&'a mut Pool);

impl<'a> Iterator for Drain<'a> {
    type Item = ();

    fn next(&mut self) -> Option<()> {
        unimplemented!();
    }
}

#[derive(Eq, Hash, PartialEq)]
struct GraphicsKey {
    graphics_mode: GraphicsMode,
    render_pass_mode: RenderPassMode,
    subpass_idx: u8,
}

pub struct Pool {
    cmd_pools: HashMap<QueueFamilyId, PoolRef<CommandPool>>,
    compilers: PoolRef<Compiler>,
    computes: HashMap<ComputeMode, PoolRef<Compute>>,
    data: HashMap<BufferUsage, PoolRef<Data>>,
    desc_pools: HashMap<DescriptorPoolKey, PoolRef<DescriptorPool>>,
    fences: PoolRef<Fence>,
    graphics: HashMap<GraphicsKey, PoolRef<Graphics>>,

    /// The number of frames which must elapse before a least-recently-used cache item is considered obsolete.
    ///
    /// Remarks: Higher numbers such as 10 will use more memory but have less thrashing than lower numbers, such as 1.
    pub lru_threshold: usize,

    memories: HashMap<MemoryTypeId, PoolRef<Memory>>,
    render_passes: HashMap<RenderPassMode, RenderPass>,
    textures: HashMap<TextureKey, PoolRef<TextureRef<Image2d>>>,
}

// TODO: Add some way to track memory usage so that using drain has some sort of feedback for users, tell them about the usage
impl Pool {
    pub(super) fn cmd_pool(
        &mut self,
        driver: &Driver,
        family: QueueFamilyId,
    ) -> Lease<CommandPool> {
        let items = self
            .cmd_pools
            .entry(family)
            .or_insert_with(Default::default);
        let mut item = if let Some(item) = items.borrow_mut().pop_back() {
            item
        } else {
            CommandPool::new(Driver::clone(driver), family)
        };

        unsafe {
            item.as_mut().reset(false);
        }

        Lease::new(item, items)
    }

    pub(super) fn compiler(&mut self) -> Lease<Compiler> {
        let item = if let Some(item) = self.compilers.borrow_mut().pop_back() {
            item
        } else {
            debug!("Creating new compiler");
            Default::default()
        };

        Lease::new(item, &self.compilers)
    }

    pub(super) fn compute(
        &mut self,
        #[cfg(debug_assertions)] name: &str,
        driver: &Driver,
        mode: ComputeMode,
    ) -> Lease<Compute> {
        self.compute_sets(
            #[cfg(debug_assertions)]
            name,
            driver,
            mode,
            1,
        )
    }

    pub(super) fn compute_sets(
        &mut self,
        #[cfg(debug_assertions)] name: &str,
        driver: &Driver,
        mode: ComputeMode,
        max_sets: usize,
    ) -> Lease<Compute> {
        let items = self.computes.entry(mode).or_insert_with(Default::default);
        let item = if let Some(item) =
            remove_last_by(&mut items.borrow_mut(), |item| item.max_sets() >= max_sets)
        {
            item
        } else {
            let ctor = match mode {
                ComputeMode::CalculateVertexAttributes => Compute::calc_vertex_attrs,
                ComputeMode::DecodeRgbRgba => Compute::decode_rgb_rgba,
            };
            ctor(
                #[cfg(debug_assertions)]
                name,
                driver,
            )
        };

        Lease::new(item, items)
    }

    pub(super) fn data(
        &mut self,
        #[cfg(debug_assertions)] name: &str,
        driver: &Driver,
        len: u64,
    ) -> Lease<Data> {
        self.data_usage(
            #[cfg(debug_assertions)]
            name,
            driver,
            len,
            BufferUsage::empty(),
        )
    }

    pub(super) fn data_usage(
        &mut self,
        #[cfg(debug_assertions)] name: &str,
        driver: &Driver,
        len: u64,
        usage: BufferUsage,
    ) -> Lease<Data> {
        let items = self.data.entry(usage).or_insert_with(Default::default);
        let item = if let Some(item) =
            remove_last_by(&mut items.borrow_mut(), |item| item.capacity() >= len)
        {
            item
        } else {
            Data::new(
                #[cfg(debug_assertions)]
                name,
                Driver::clone(driver),
                len,
                usage,
            )
        };

        Lease::new(item, items)
    }

    // TODO: I don't really like the function signature here
    pub(super) fn desc_pool<'i, I>(
        &mut self,
        driver: &Driver,
        max_sets: usize,
        desc_ranges: I,
    ) -> Lease<DescriptorPool>
    where
        I: Clone + ExactSizeIterator<Item = &'i DescriptorRangeDesc>,
    {
        let desc_ranges_key = desc_ranges
            .clone()
            .map(|desc_range| (desc_range.ty, desc_range.count))
            .collect();
        // TODO: Sort (and possibly combine) desc_ranges so that different orders of the same data don't affect key lookups
        let items = self
            .desc_pools
            .entry(DescriptorPoolKey {
                desc_ranges: desc_ranges_key,
            })
            .or_insert_with(Default::default);
        let item = if let Some(item) = remove_last_by(&mut items.borrow_mut(), |item| {
            DescriptorPool::max_sets(&item) >= max_sets
        }) {
            item
        } else {
            DescriptorPool::new(Driver::clone(driver), max_sets, desc_ranges)
        };

        Lease::new(item, items)
    }

    /// Allows callers to remove unused memory-consuming items from the pool.
    pub fn drain(&mut self) -> Drain {
        Drain(self)
    }

    pub(super) fn fence(
        &mut self,
        #[cfg(debug_assertions)] name: &str,
        driver: &Driver,
    ) -> Lease<Fence> {
        let item = if let Some(mut item) = self.fences.borrow_mut().pop_back() {
            Fence::reset(&mut item);
            item
        } else {
            Fence::new(
                #[cfg(debug_assertions)]
                name,
                Driver::clone(driver),
            )
        };

        Lease::new(item, &self.fences)
    }

    pub(super) fn graphics(
        &mut self,
        #[cfg(debug_assertions)] name: &str,
        driver: &Driver,
        graphics_mode: GraphicsMode,
        render_pass_mode: RenderPassMode,
        subpass_idx: u8,
    ) -> Lease<Graphics> {
        self.graphics_sets(
            #[cfg(debug_assertions)]
            name,
            driver,
            graphics_mode,
            render_pass_mode,
            subpass_idx,
            1,
        )
    }

    pub(super) fn graphics_sets(
        &mut self,
        #[cfg(debug_assertions)] name: &str,
        driver: &Driver,
        graphics_mode: GraphicsMode,
        render_pass_mode: RenderPassMode,
        subpass_idx: u8,
        max_sets: usize,
    ) -> Lease<Graphics> {
        {
            let items = self
                .graphics
                .entry(GraphicsKey {
                    graphics_mode,
                    render_pass_mode,
                    subpass_idx,
                })
                .or_insert_with(Default::default);
            if let Some(item) =
                remove_last_by(&mut items.borrow_mut(), |item| item.max_sets() >= max_sets)
            {
                return Lease::new(item, items);
            }
        }
        let ctor = match graphics_mode {
            GraphicsMode::Blend(BlendMode::Normal) => Graphics::blend_normal,
            GraphicsMode::Blend(_) => todo!(),
            GraphicsMode::DrawLine => Graphics::draw_line,
            GraphicsMode::DrawMesh => Graphics::draw_mesh,
            GraphicsMode::DrawPointLight => Graphics::draw_point_light,
            GraphicsMode::DrawRectLight => Graphics::draw_rect_light,
            GraphicsMode::DrawSpotlight => Graphics::draw_spotlight,
            GraphicsMode::DrawSunlight => Graphics::draw_sunlight,
            GraphicsMode::Font => Graphics::font,
            GraphicsMode::FontOutline => Graphics::font_outline,
            GraphicsMode::Gradient => Graphics::gradient,
            GraphicsMode::GradientTransparency => Graphics::gradient_transparency,
            GraphicsMode::Texture => Graphics::texture,
        };
        let item = unsafe {
            ctor(
                #[cfg(debug_assertions)]
                name,
                driver,
                RenderPass::subpass(self.render_pass(driver, render_pass_mode), subpass_idx),
                max_sets,
            )
        };

        let items = &self.graphics[&GraphicsKey {
            graphics_mode,
            render_pass_mode,
            subpass_idx,
        }];
        Lease::new(item, items)
    }

    pub(super) fn memory(
        &mut self,
        driver: &Driver,
        mem_type: MemoryTypeId,
        size: u64,
    ) -> Lease<Memory> {
        let items = self
            .memories
            .entry(mem_type)
            .or_insert_with(Default::default);
        let item = if let Some(item) =
            remove_last_by(&mut items.borrow_mut(), |item| Memory::size(&item) >= size)
        {
            item
        } else {
            Memory::new(Driver::clone(driver), mem_type, size)
        };

        Lease::new(item, items)
    }

    pub(super) fn render_pass(&mut self, driver: &Driver, mode: RenderPassMode) -> &RenderPass {
        let driver = Driver::clone(driver);
        self.render_passes
            .entry(mode)
            .or_insert_with(|| match mode {
                RenderPassMode::Color(mode) => color(driver, mode),
                RenderPassMode::Draw(mode) => draw(driver, mode),
            })
    }

    // TODO: Bubble format picking up and out of this! (removes desire_tiling+desired_fmts+features, replace with fmt/tiling)
    #[allow(clippy::too_many_arguments)]
    pub(super) fn texture(
        &mut self,
        #[cfg(debug_assertions)] name: &str,
        driver: &Driver,
        dims: Extent,
        fmt: Format,
        layout: Layout,
        usage: ImageUsage,
        layers: u16,
        mips: u8,
        samples: u8,
    ) -> Lease<TextureRef<Image2d>> {
        let items = self
            .textures
            .entry(TextureKey {
                dims,
                fmt,
                layers,
                mips,
                samples,
                usage,
            })
            .or_insert_with(Default::default);
        let item = {
            let mut items_ref = items.as_ref().borrow_mut();
            if let Some(item) = items_ref.pop_back() {
                // Set a new name on this texture
                #[cfg(debug_assertions)]
                unsafe {
                    driver
                        .as_ref()
                        .borrow()
                        .set_image_name(item.as_ref().borrow_mut().as_mut(), name);
                }

                item
            } else {
                // Add a cache item so there will be an unused item waiting next time
                items_ref.push_front(TextureRef::new(RefCell::new(Texture::new(
                    #[cfg(debug_assertions)]
                    &format!("{} (Unused)", name),
                    Driver::clone(driver),
                    dims,
                    fmt,
                    layout,
                    usage,
                    layers,
                    samples,
                    mips,
                ))));

                // Return a brand new instance
                TextureRef::new(RefCell::new(Texture::new(
                    #[cfg(debug_assertions)]
                    name,
                    Driver::clone(driver),
                    dims,
                    fmt,
                    layout,
                    usage,
                    layers,
                    samples,
                    mips,
                )))
            }
        };

        Lease::new(item, items)
    }
}

impl Default for Pool {
    fn default() -> Self {
        Self {
            cmd_pools: Default::default(),
            compilers: Default::default(),
            computes: Default::default(),
            data: Default::default(),
            desc_pools: Default::default(),
            fences: Default::default(),
            graphics: Default::default(),
            lru_threshold: DEFAULT_LRU_THRESHOLD,
            memories: Default::default(),
            render_passes: Default::default(),
            textures: Default::default(),
        }
    }
}

#[derive(Eq, Hash, PartialEq)]
struct TextureKey {
    dims: Extent,
    fmt: Format,
    layers: u16,
    mips: u8,
    samples: u8,
    usage: ImageUsage, // TODO: Usage shouldn't be a hard filter like this
}
