//! # OptiX
//!
//! <div style = "background-color: #fff7e1; padding: 0; margin-bottom: 1em">
//! <span style="float:left; font-size: 4em; padding-left: 0.25em; padding-right: 0.25em;">!</span>
//! <p style = "padding: 1em">
//! You must call <code>optix::init()</code> before calling any of the functions
//! in this crate in order to load the necessary symbols from the driver.
//! </p>
//! </div>
//!
//! Rust bindings for NVIDIA's OptiX GPU raytracing library.
//!
//!  NVIDIA OptiX 7 is intended for ray tracing applications that use NVIDIA® CUDA®
//!  technology, such as:
//!
//! * Film and television visual effects
//! * Computer-aided design for engineering and manufacturing
//! * Light maps generated by path tracing
//! * High-performance computing
//! * LIDAR simulation
//!
//! NVIDIA OptiX 7 also includes support for motion blur and multi-level transforms,
//! features required by ray-tracing applications designed for production-quality
//! rendering.
//!
//! # Programming Guide
//!
//! For high-level documentation please see the
//! [introduction](crate::introduction) module documentation and subsequent documentation in the
//! modules listed below. Each module has an expandable "Programming Guide" section that will
//! display the docs when clicked.
//!
//! * [1. Introduction](introduction)
//! * [2. Context](context)
//! * [3. Acceleration Structures](acceleration)
//! * [4. Program Pipeline Creation](pipeline)
//! * [5. Shader Binding Table](shader_binding_table)
//! * [6. Ray Generation Launches](launch)
//!

#[doc = ::embed_doc_image::embed_image!("optix_programs", "images/optix_programs.jpg")]
#[doc = ::embed_doc_image::embed_image!("traversables_graph", "images/traversables_graph.jpg")]
#[doc = include_str!("introduction.md")]
pub mod introduction {}

#[doc = include_str!("acceleration.md")]
pub mod acceleration;

#[doc = include_str!("context.md")]
pub mod context;

#[doc = include_str!("denoiser.md")]
pub mod denoiser;

/// Error handling
pub mod error;

#[doc = include_str!("pipeline.md")]
pub mod pipeline;
pub mod prelude;

#[doc = ::embed_doc_image::embed_image!("example_sbt", "images/example_sbt.png")]
#[doc = ::embed_doc_image::embed_image!("scene_graph", "images/scene_graph.png")]
#[doc = include_str!("shader_binding_table.md")]
pub mod shader_binding_table;
use shader_binding_table::ShaderBindingTable;

pub use cust;
use cust::memory::DeviceMemory;
use error::{Error, ToResult};
type Result<T, E = Error> = std::result::Result<T, E>;

/// Initializes the OptiX library. This must be called before using any OptiX function. It may
/// be called before or after initializing CUDA.
pub fn init() -> Result<()> {
    // avoid initializing multiple times because that will try to load the dll every time.
    if !optix_is_initialized() {
        init_cold()
    } else {
        Ok(())
    }
}

#[cold]
#[inline(never)]
fn init_cold() -> Result<()> {
    unsafe { Ok(optix_sys::optixInit().to_result()?) }
}

/// Whether OptiX is initialized. If you are calling raw [`sys`] functions you must make sure
/// this is true, otherwise OptiX will segfault. In the safe wrapper it is done automatically and optix not
/// being initialized will return an error result.
#[doc(hidden)]
pub fn optix_is_initialized() -> bool {
    // SAFETY: C globals are explicitly defined to be zero-initialized, and the sys version uses
    // Option for each field, and None is explicitly defined to be represented as a nullptr for Option<fn()>,
    // so its default should be the same as the zero-initialized global.
    // And, while we do not currently expose it, optix library unloading zero initializes the global.
    unsafe { g_optixFunctionTable != optix_sys::OptixFunctionTable::default() }
}

extern "C" {
    pub(crate) static g_optixFunctionTable_105: optix_sys::OptixFunctionTable;
}
#[allow(non_upper_case_globals)]
pub(crate) use g_optixFunctionTable_105 as g_optixFunctionTable;

/// Call a raw OptiX sys function, making sure that OptiX is initialized. Returning
/// an OptixNotInitialized error if it is not initialized. See [`optix_is_initialized`].
#[doc(hidden)]
#[macro_export]
macro_rules! optix_call {
    ($name:ident($($param:expr),* $(,)?)) => {{
          if !$crate::optix_is_initialized() {
              Err($crate::error::OptixError::OptixNotInitialized)
          } else {
              <optix_sys::OptixResult as $crate::error::ToResult>::to_result(optix_sys::$name($($param),*))
          }
    }};
}

/// Launch the given [`Pipeline`](pipeline::Pipeline) on the given [`Stream`](cust::stream::Stream).
///
/// A ray generation launch is the primary workhorse of the NVIDIA OptiX API. A
/// launch invokes a 1D, 2D or 3D array of threads on the device and invokes ray
/// generation programs for each thread. When the ray generation program invokes
/// `optixTrace`, other programs are invoked to execute traversal, intersection,
/// any-hit, closest-hit, miss and exception programs until the invocations are
/// complete.
///
/// A pipeline requires device-side memory for each launch. This space is allocated
/// and managed by the API. Because launch resources may be shared between pipelines,
/// they are only guaranteed to be freed when the [`DeviceContext`] is destroyed.
///
/// All launches are asynchronous, using [`CUDA stream`]s. When it is necessary
/// to implement synchronization, use the mechanisms provided by CUDA streams and
/// events.
///
/// In addition to the pipeline object, the CUDA stream, and the launch state, it
/// is necessary to provide information about the SBT layout using the
/// [`ShaderBindingTable`](crate::shader_binding_table::ShaderBindingTable) struct
/// (see [Shader Binding Table](crate::shader_binding_table)).
///
/// The value of the pipeline launch parameter is specified by the
/// `pipeline_launch_params_variable_name` field of the
/// [`PipelineCompileOptions`](crate::pipeline::PipelineCompileOptions) struct.
/// It is determined at launch with a [`DevicePointer`](cust::memory::DevicePointer)
/// parameter, named `pipeline_params`]. This must be the same size as that passed
/// to the module compilation or an error will occur.
///
/// The kernel creates a copy of `pipeline_params` before the launch, so the kernel
/// is allowed to modify `pipeline_params` values during the launch. This means
/// that subsequent launches can run with modified pipeline parameter values. Users
/// cannot synchronize with this copy between the invocation of `launch()` and
/// the start of the kernel.
///
/// # Safety
/// You must ensure that:
/// - Any device memory referenced in `buf_launch_params` point to valid,
///   correctly aligned memory
/// - Any [`SbtRecord`](shader_binding_table::SbtRecord)s and associated data
///     referenced by the
///     [`ShaderBindingTable`](shader_binding_table::ShaderBindingTable) are alive
///     and valid
///
/// [`CUDA stream`]: cust::stream::Stream
/// [`DeviceContext`]: crate::context::DeviceContext
pub unsafe fn launch<M: DeviceMemory>(
    pipeline: &crate::pipeline::Pipeline,
    stream: &cust::stream::Stream,
    pipeline_params: &M,
    sbt: &ShaderBindingTable,
    width: u32,
    height: u32,
    depth: u32,
) -> Result<()> {
    Ok(optix_call!(optixLaunch(
        pipeline.raw,
        stream.as_inner(),
        pipeline_params.as_raw_ptr(),
        pipeline_params.size_in_bytes(),
        &sbt.0,
        width,
        height,
        depth,
    ))?)
}

#[cfg(feature = "glam")]
mod impl_glam;

macro_rules! const_assert {
    ($x:expr $(,)?) => {
        #[allow(unknown_lints, clippy::eq_op)]
        const _: [(); 0 - !{
            const ASSERT: bool = $x;
            ASSERT
        } as usize] = [];
    };
}
pub(crate) use const_assert;

macro_rules! const_assert_eq {
    ($x:expr, $y:expr $(,)?) => {
        const_assert!($x == $y);
    };
}
pub(crate) use const_assert_eq;
