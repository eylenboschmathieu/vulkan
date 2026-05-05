#![allow(dead_code, unsafe_op_in_unsafe_fn)]

use crate::{
    commands::CommandManager,
    device::Device,
    instance::Instance,
    pipeline::{
        descriptor_set_layout::DescriptorSetLayout,
        descriptors::DescriptorPool,
    },
    queues::QueueManager,
    resources::{
        buffers::{
            index_buffer::IndexBuffer,
            staging_buffer::StagingBuffer,
            uniform_buffer::UniformBuffer,
            vertex_buffer::VertexBuffer,
        },
        image::Textures,
    },
};

macro_rules! global {
    ($static:ident, $ty:ty, $get:ident, $get_mut:ident, $init:ident) => {
        static mut $static: Option<$ty> = None;

        pub unsafe fn $get() -> &'static $ty {
            match &*std::ptr::addr_of!($static) {
                Some(val) => val,
                None => panic!(concat!(stringify!($ty), " not initialized")),
            }
        }

        pub unsafe fn $get_mut() -> &'static mut $ty {
            match &mut *std::ptr::addr_of_mut!($static) {
                Some(val) => val,
                None => panic!(concat!(stringify!($ty), " not initialized")),
            }
        }

        pub unsafe fn $init(val: $ty) {
            *std::ptr::addr_of_mut!($static) = Some(val);
        }
    };
}

global!(INSTANCE,            Instance,            instance,            instance_mut,            init_instance);
global!(DEVICE,              Device,              device,              device_mut,              init_device);
global!(QUEUE_MANAGER,       QueueManager,        queue_manager,       queue_manager_mut,       init_queue_manager);
global!(COMMAND_MANAGER,     CommandManager,      command_manager,     command_manager_mut,     init_command_manager);
global!(STAGING_BUFFER,      StagingBuffer,       staging_buffer,      staging_buffer_mut,      init_staging_buffer);
global!(INDEX_BUFFER,        IndexBuffer,         index_buffer,        index_buffer_mut,        init_index_buffer);
global!(VERTEX_BUFFER,       VertexBuffer,        vertex_buffer,       vertex_buffer_mut,       init_vertex_buffer);
global!(UNIFORM_BUFFER,      UniformBuffer,       uniform_buffer,      uniform_buffer_mut,      init_uniform_buffer);
global!(DESCRIPTOR_POOL,     DescriptorPool,      descriptor_pool,     descriptor_pool_mut,     init_descriptor_pool);
global!(DSL,                 DescriptorSetLayout, descriptor_set_layouts, descriptor_set_layouts_mut, init_descriptor_set_layouts);
global!(TEXTURES,            Textures,            textures,            textures_mut,            init_textures);