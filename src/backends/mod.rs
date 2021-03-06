// Copyright (c) 2017 King's College London
// created by the Software Development Team <http://soft-dev.org/>
//
// The Universal Permissive License (UPL), Version 1.0
//
// Subject to the condition set forth below, permission is hereby granted to any
// person obtaining a copy of this software, associated documentation and/or
// data (collectively the "Software"), free of charge and under any and all
// copyright rights in the Software, and any and all patent rights owned or
// freely licensable by each licensor hereunder covering either (i) the
// unmodified Software as contributed to or provided by such licensor, or (ii)
// the Larger Works (as defined below), to deal in both
//
// (a) the Software, and
// (b) any piece of software and/or hardware listed in the lrgrwrks.txt file
// if one is included with the Software (each a "Larger Work" to which the Software
// is contributed by such licensors),
//
// without restriction, including without limitation the rights to copy, create
// derivative works of, display, perform, and distribute the Software and make,
// use, sell, offer for sale, import, export, have made, and have sold the
// Software and the Larger Work(s), and to sublicense the foregoing rights on
// either these or other terms.
//
// This license is subject to the following condition: The above copyright
// notice and either this complete permission notice or at a minimum a reference
// to the UPL must be included in all copies or substantial portions of the
// Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

use errors::HWTracerError;
use Tracer;
use backends::dummy::DummyTracer;

#[cfg(perf_pt)]
pub mod perf_pt;
#[cfg(perf_pt)]
use backends::perf_pt::PerfPTTracer;
use libc::size_t;
pub mod dummy;

#[derive(Debug)]
pub enum BackendKind {
    Dummy,
    PerfPT,
}

const PERF_PT_DFLT_DATA_BUFSIZE: size_t = 64;
const PERF_PT_DFLT_AUX_BUFSIZE: size_t = 1024;
const PERF_PT_DFLT_INITIAL_TRACE_BUFSIZE: size_t = 1024 * 1024; // 1MiB

impl BackendKind {
    // Finds a suitable `BackendKind` for the current hardware/OS.
    fn default_platform_backend() -> BackendKind {
        let tr_kinds = vec![BackendKind::PerfPT, BackendKind::Dummy];
        for kind in tr_kinds {
            if Self::match_platform(&kind).is_ok() {
                return kind;
            }
        }
        // The Dummy backend should always be usable.
        unreachable!();
    }

    /// Returns `Ok` if the this backend is appropriate for the current platform.
    fn match_platform(&self) -> Result<(), HWTracerError> {
        match self {
            BackendKind::Dummy => Ok(()),
            BackendKind::PerfPT => {
                #[cfg(not(perf_pt))]
                return Err(HWTracerError::BackendUnavailable(BackendKind::PerfPT));
                #[cfg(perf_pt)] {
                    if !Self::pt_supported() {
                        return Err(HWTracerError::NoHWSupport("Intel PT not supported by CPU".into()));
                    }
                    Ok(())
                }
            }
        }
    }

    /// Checks if the CPU supports Intel Processor Trace.
    #[cfg(perf_pt)]
    fn pt_supported() -> bool {
        const LEAF: u32 = 0x07;
        const SUBPAGE: u32 = 0x0;
        const EBX_BIT: u32 = 1 << 25;
        let ebx_out: u32;

        unsafe {
            asm!(r"
                  mov $1, %eax;
                  mov $2, %ecx;
                  cpuid;"
                : "={ebx}" (ebx_out)
                : "i" (LEAF), "i" (SUBPAGE)
                : "eax", "ecx", "edx"
                : "volatile");
        }
        ebx_out & EBX_BIT != 0
    }
}

/// Generic configuration interface for all backends.
/// If a field is `None` at `build()` time then the backend will select a default value. Ant
/// attributes which don't apply to a given backend are also checked.
#[derive(Debug)]
pub enum BackendConfig {
    Dummy,
    PerfPT(PerfPTConfig),
}


/// Configures the PerfPT backend.
///
// Must stay in sync with the C code.
#[derive(Clone, Debug)]
#[repr(C)]
pub struct PerfPTConfig {
    /// Data buffer size, in pages. Must be a power of 2.
    pub data_bufsize: size_t,
    /// AUX buffer size, in pages. Must be a power of 2.
    pub aux_bufsize: size_t,
    /// The initial trace storage buffer size (in bytes) of new traces.
    pub initial_trace_bufsize: size_t,
}

impl Default for PerfPTConfig {
    fn default() -> Self {
        Self {
            data_bufsize: PERF_PT_DFLT_DATA_BUFSIZE,
            aux_bufsize: PERF_PT_DFLT_AUX_BUFSIZE,
            initial_trace_bufsize: PERF_PT_DFLT_INITIAL_TRACE_BUFSIZE,
        }
    }
}

impl BackendConfig {
    fn backend_kind(&self) -> BackendKind {
        match self {
            BackendConfig::Dummy => BackendKind::Dummy,
            BackendConfig::PerfPT{..} => BackendKind::PerfPT,
        }
    }
}

/// A builder interface for instantiating `Tracer`s.
///
/// # Make a tracer with an appropriate default backend and using default backend options.
/// ```
/// use hwtracer::backends::TracerBuilder;
/// TracerBuilder::new().build().unwrap();
/// ```
///
/// # Make a tracer using the PerfPT backend and using the default backend options.
/// ```
/// use hwtracer::backends::TracerBuilder;
///
/// let res = TracerBuilder::new().perf_pt().build();
/// if let Ok(tracer) = res {
///     // Use the tracer...
/// } else {
///     // CPU doesn't support Intel Processor Trace.
/// }
/// ```
///
/// # Make a tracer with an appropriate default backend and using custom backend options if the PerfPT backend was chosen.
/// ```
/// use hwtracer::backends::{TracerBuilder, BackendConfig};
/// let mut bldr = TracerBuilder::new();
/// if let BackendConfig::PerfPT(ref mut ppt_config) = bldr.config() {
///     ppt_config.aux_bufsize = 8192;
/// }
/// bldr.build().unwrap();
/// ```
pub struct TracerBuilder {
    config: BackendConfig,
}

impl TracerBuilder {
    /// Create a new TracerBuilder using an appropriate default backend and configuration.
    pub fn new() -> Self {
        let config = match BackendKind::default_platform_backend() {
            BackendKind::Dummy => BackendConfig::Dummy,
            BackendKind::PerfPT => BackendConfig::PerfPT(PerfPTConfig::default()),
        };
        Self{config}
    }

    /// Choose to use the PerfPT backend wth default options.
    pub fn perf_pt(mut self) -> Self {
        self.config = BackendConfig::PerfPT(PerfPTConfig::default());
        self
    }

    /// Choose to use the Dummy backend.
    pub fn dummy(mut self) -> Self {
        self.config = BackendConfig::Dummy;
        self
    }

    /// Get a mutable reference to the configuraion.
    pub fn config(&mut self) -> &mut BackendConfig {
        &mut self.config
    }

    /// Build a tracer from the specified configuration.
    /// An error is returned if the requested backend is inappropriate for the platform or the
    /// requested backend was not compiled in to hwtracer.
    pub fn build(self) -> Result<Box<dyn Tracer>, HWTracerError> {
        let backend_kind = self.config.backend_kind();
        backend_kind.match_platform()?;
        match self.config {
            BackendConfig::PerfPT(_pt_conf) => {  // _pt_conf will be unused if perf_pt wasn't built in.
                #[cfg(perf_pt)]
                return Ok(Box::new(PerfPTTracer::new(_pt_conf)?));
                #[cfg(not(perf_pt))]
                unreachable!();
            },
            BackendConfig::Dummy => return Ok(Box::new(DummyTracer::new())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{TracerBuilder, BackendConfig};

    // Check that building a default Tracer works.
    #[test]
    fn test_builder_default_backend() {
        assert!(TracerBuilder::new().build().is_ok());
    }

    // Check we can conditionally configure an automatically chosen backend.
    #[test]
    fn test_builder_configure_default_backend() {
        let mut bldr = TracerBuilder::new();
        if let BackendConfig::PerfPT(ref mut ppt_config) = bldr.config() {
            ppt_config.aux_bufsize = 8192;
        }
        assert!(bldr.build().is_ok());
    }

    // Check the `TracerBuilder` correctly reports an unavailable backend.
    #[cfg(not(perf_pt))]
    #[test]
    fn test_backend_unavailable() {
        match TracerBuilder::new().perf_pt().build() {
            Ok(_) => panic!("backend should be unavailable"),
            Err(e) => assert_eq!(e.to_string(), "Backend unavailble: PerfPT"),
        }
    }

    // Ensure we can share `Tracer`s between threads.
    #[test]
    fn test_shared_tracers_betwen_threads() {
        use std::sync::Arc;
        use std::thread;
        let arc1 = Arc::new(TracerBuilder::new().build().unwrap());
        let arc2 = Arc::clone(&arc1);

        thread::spawn(move || {
            let _ = arc2;
        }).join().unwrap();
    }
}
