//! Draws every vertex in one call. The sphere mesh is instanced once per
//! vertex, with the position and color of each instance in a GPU
//! buffer. Adapted from Bevy's `custom_shader_instancing` example.

use std::sync::Arc;

use bevy::asset::{load_internal_asset, uuid_handle};
use bevy::core_pipeline::core_3d::Transparent3d;
use bevy::ecs::system::SystemParamItem;
use bevy::ecs::system::lifetimeless::{Read, SRes};
use bevy::mesh::{MeshVertexBufferLayoutRef, VertexBufferLayout};
use bevy::pbr::{
    MeshPipeline, MeshPipelineKey, RenderMeshInstances, SetMeshBindGroup, SetMeshViewBindGroup,
    SetMeshViewBindingArrayBindGroup,
};
use bevy::prelude::*;
use bevy::render::extract_component::{ExtractComponent, ExtractComponentPlugin};
use bevy::render::mesh::allocator::MeshAllocator;
use bevy::render::mesh::{RenderMesh, RenderMeshBufferInfo};
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_phase::{
    AddRenderCommand, DrawFunctions, PhaseItem, PhaseItemExtraIndex, RenderCommand, RenderCommandResult,
    SetItemPipeline, TrackedRenderPass, ViewSortedRenderPhases,
};
use bevy::render::render_resource::*;
use bevy::render::renderer::RenderDevice;
use bevy::render::sync_world::MainEntity;
use bevy::render::view::ExtractedView;
use bevy::render::{Render, RenderApp, RenderStartup, RenderSystems};
use bytemuck::{Pod, Zeroable};

const SHADER: Handle<Shader> = uuid_handle!("5f1b0c2e-8d4a-4f6b-9c3e-2a7d8e9f0b1c");

/// One drawn vertex: 28 bytes, against kilobytes for an entity per vertex.
#[derive(Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct Instance {
    pub position: Vec3,
    pub color: [f32; 4],
}

/// The instances, shared with the render world through the `Arc`, so that
/// extracting them every frame copies nothing.
#[derive(Component, Clone, Default, ExtractComponent)]
pub struct VertexInstances(pub Arc<Vec<Instance>>);

pub struct InstancingPlugin;

impl Plugin for InstancingPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(app, SHADER, "vertices.wgsl", Shader::from_wgsl);
        app.add_plugins(ExtractComponentPlugin::<VertexInstances>::default());
        app.sub_app_mut(RenderApp)
            .add_render_command::<Transparent3d, DrawVertices>()
            .init_resource::<SpecializedMeshPipelines<VertexPipeline>>()
            .add_systems(RenderStartup, init_pipeline)
            .add_systems(
                Render,
                (
                    queue_vertices.in_set(RenderSystems::QueueMeshes),
                    prepare_buffers.in_set(RenderSystems::PrepareResources),
                ),
            );
    }
}

fn queue_vertices(
    draw_functions: Res<DrawFunctions<Transparent3d>>,
    vertex_pipeline: Res<VertexPipeline>,
    mut pipelines: ResMut<SpecializedMeshPipelines<VertexPipeline>>,
    pipeline_cache: Res<PipelineCache>,
    meshes: Res<RenderAssets<RenderMesh>>,
    mesh_instances: Res<RenderMeshInstances>,
    entities: Query<(Entity, &MainEntity), With<VertexInstances>>,
    mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>,
    views: Query<(&ExtractedView, &Msaa)>,
) {
    let draw = draw_functions.read().id::<DrawVertices>();

    for (view, msaa) in &views {
        let Some(phase) = phases.get_mut(&view.retained_view_entity) else {
            continue;
        };
        let view_key = MeshPipelineKey::from_msaa_samples(msaa.samples()) | MeshPipelineKey::from_hdr(view.hdr);
        let rangefinder = view.rangefinder3d();

        for (entity, main_entity) in &entities {
            let Some(mesh_instance) = mesh_instances.render_mesh_queue_data(*main_entity) else {
                continue;
            };
            let Some(mesh) = meshes.get(mesh_instance.mesh_asset_id) else {
                continue;
            };
            let key = view_key | MeshPipelineKey::from_primitive_topology(mesh.primitive_topology());
            let Ok(pipeline) = pipelines.specialize(&pipeline_cache, &vertex_pipeline, key, &mesh.layout) else {
                continue;
            };

            phase.add(Transparent3d {
                entity: (entity, *main_entity),
                pipeline,
                draw_function: draw,
                distance: rangefinder.distance(&mesh_instance.center),
                batch_range: 0..1,
                extra_index: PhaseItemExtraIndex::None,
                indexed: true,
            });
        }
    }
}

#[derive(Component)]
struct InstanceBuffer {
    buffer: Buffer,
    length: usize,
    /// The instances uploaded, to upload again only when they change.
    source: Arc<Vec<Instance>>,
}

fn prepare_buffers(
    mut commands: Commands,
    entities: Query<(Entity, &VertexInstances, Option<&InstanceBuffer>)>,
    device: Res<RenderDevice>,
) {
    for (entity, instances, uploaded) in &entities {
        if uploaded.is_some_and(|uploaded| Arc::ptr_eq(&uploaded.source, &instances.0)) {
            continue;
        }
        if instances.0.is_empty() {
            commands.entity(entity).remove::<InstanceBuffer>();
            continue;
        }

        let buffer = device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("vertex instances"),
            contents: bytemuck::cast_slice(&instances.0),
            usage: BufferUsages::VERTEX,
        });
        let length = instances.0.len();
        commands.entity(entity).insert(InstanceBuffer { buffer, length, source: instances.0.clone() });
    }
}

#[derive(Resource)]
struct VertexPipeline {
    mesh_pipeline: MeshPipeline,
}

fn init_pipeline(mut commands: Commands, mesh_pipeline: Res<MeshPipeline>) {
    commands.insert_resource(VertexPipeline { mesh_pipeline: mesh_pipeline.clone() });
}

impl SpecializedMeshPipeline for VertexPipeline {
    type Key = MeshPipelineKey;

    fn specialize(
        &self,
        key: Self::Key,
        layout: &MeshVertexBufferLayoutRef,
    ) -> Result<RenderPipelineDescriptor, SpecializedMeshPipelineError> {
        let mut descriptor = self.mesh_pipeline.specialize(key, layout)?;

        // Locations 0 to 2 are the position, normal and uv of the mesh.
        descriptor.vertex.shader = SHADER;
        descriptor.vertex.buffers.push(VertexBufferLayout {
            array_stride: size_of::<Instance>() as u64,
            step_mode: VertexStepMode::Instance,
            attributes: vec![
                VertexAttribute { format: VertexFormat::Float32x3, offset: 0, shader_location: 3 },
                VertexAttribute { format: VertexFormat::Float32x4, offset: 12, shader_location: 4 },
            ],
        });
        descriptor.fragment.as_mut().expect("the mesh pipeline has a fragment stage").shader = SHADER;
        Ok(descriptor)
    }
}

type DrawVertices = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetMeshViewBindingArrayBindGroup<1>,
    SetMeshBindGroup<2>,
    DrawInstances,
);

struct DrawInstances;

impl<P: PhaseItem> RenderCommand<P> for DrawInstances {
    type Param = (SRes<RenderAssets<RenderMesh>>, SRes<RenderMeshInstances>, SRes<MeshAllocator>);
    type ViewQuery = ();
    type ItemQuery = Read<InstanceBuffer>;

    fn render<'w>(
        item: &P,
        _view: (),
        instances: Option<&'w InstanceBuffer>,
        (meshes, mesh_instances, allocator): SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let allocator = allocator.into_inner();
        let Some(mesh_instance) = mesh_instances.render_mesh_queue_data(item.main_entity()) else {
            return RenderCommandResult::Skip;
        };
        let (Some(mesh), Some(instances), Some(vertices)) = (
            meshes.into_inner().get(mesh_instance.mesh_asset_id),
            instances,
            allocator.mesh_vertex_slice(&mesh_instance.mesh_asset_id),
        ) else {
            return RenderCommandResult::Skip;
        };

        pass.set_vertex_buffer(0, vertices.buffer.slice(..));
        pass.set_vertex_buffer(1, instances.buffer.slice(..));
        let count = 0..instances.length as u32;

        match &mesh.buffer_info {
            RenderMeshBufferInfo::Indexed { index_format, count: indices } => {
                let Some(index) = allocator.mesh_index_slice(&mesh_instance.mesh_asset_id) else {
                    return RenderCommandResult::Skip;
                };
                pass.set_index_buffer(index.buffer.slice(..), *index_format);
                pass.draw_indexed(index.range.start..index.range.start + indices, vertices.range.start as i32, count);
            }
            RenderMeshBufferInfo::NonIndexed => pass.draw(vertices.range, count),
        }

        RenderCommandResult::Success
    }
}
