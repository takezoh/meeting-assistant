use std::marker::PhantomData;

use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};

/// Enters the current thread into the COM multithreaded apartment and leaves it on drop.
/// It is bound to the thread, so it cannot be sent to another thread.
pub struct ComApartment(PhantomData<*const ()>);

impl ComApartment {
    pub fn enter() -> anyhow::Result<Self> {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok()?;
        Ok(Self(PhantomData))
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}
