use crate::engine::Engine;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::pin::Pin;

#[cxx_qt::bridge(namespace = "dock")]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        #[namespace = ""]
        type QString = cxx_qt_lib::QString;
    }
    extern "RustQt" {
        #[qobject]
        type RustEngine = super::RustEngineRust;
        #[qinvokable]
        pub fn dispatch(self: Pin<&mut RustEngine>, request: &QString) -> QString;
    }
}
#[derive(Default)]
pub struct RustEngineRust {
    engine: Option<Engine>,
}
impl ffi::RustEngine {
    pub fn dispatch(mut self: Pin<&mut Self>, request: &QString) -> QString {
        let mut state = self.as_mut().rust_mut();
        let output = crate::engine::dispatch(&mut state.engine, &request.to_string());
        QString::from(output.as_str())
    }
}
