use wdk_sys::{macros, NTSTATUS, WDFWAITLOCK, WDF_OBJECT_ATTRIBUTES};

use crate::nt_success;

// private module + public re-export avoids the module name repetition: https://github.com/rust-lang/rust-clippy/issues/8524
#[allow(clippy::module_name_repetitions)]

/// WDF wait Lock.
///
/// Use framework wait locks to synchronize access to driver data from code that
/// runs at `IRQL` < `DISPATCH_LEVEL`.
///
/// Before a driver can use a framework wait lock it must call
/// [`WaitLock::try_new()`] to create a [`WaitLock`]. The driver can then call
/// [`WaitLock::with()`] to acquire the lock and perform an operation using the
/// data within.
pub struct WaitLock<T> {
    wdf_wait_lock: WDFWAITLOCK,
    inner: core::cell::UnsafeCell<T>,
}

impl<T> core::ops::Drop for WaitLock<T> {
    fn drop(&mut self) {
        let _ = unsafe {
            macros::call_unsafe_wdf_function_binding! {
                WdfWaitLockRelease(self.wdf_wait_lock)
            }
        };
    }
}

impl<T> WaitLock<T> {
    /// Try to construct a WDF Wait Lock object
    ///
    /// # Errors
    ///
    /// This function will return an error if WDF fails to contruct a timer. The error variant will contain a [`NTSTATUS`] of the failure. Full error documentation is available in the [WDFwaitLock Documentation](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdfsync/nf-wdfsync-wdfwaitlockcreate#return-value)
    pub fn try_new(init: T, attributes: &mut WDF_OBJECT_ATTRIBUTES) -> Result<Self, NTSTATUS> {
        let mut wait_lock = Self {
            wdf_wait_lock: core::ptr::null_mut(),
            inner: core::cell::UnsafeCell::new(init),
        };
        // SAFETY: The resulting ffi object is stored in a private member and not
        // accessible outside of this module, and this module guarantees that it is
        // always in a valid state.
        let nt_status = unsafe {
            macros::call_unsafe_wdf_function_binding! {
                WdfWaitLockCreate(
                    attributes,
                    &mut wait_lock.wdf_wait_lock)
            }
        };
        nt_success(nt_status).then_some(wait_lock).ok_or(nt_status)
    }

    /// Try to construct a WDF Wait Lock object. This is an alias for
    /// [`WaitLock::try_new()`]
    ///
    /// # Errors
    ///
    /// This function will return an error if WDF fails to contruct a timer. The
    /// error variant will contain a [`NTSTATUS`] of the failure. Full error
    /// documentation is available in the
    /// [WDFWaitLock Documentation](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdfsync/nf-wdfsync-wdfwaitlockcreate)
    pub fn create(init: T, attributes: &mut WDF_OBJECT_ATTRIBUTES) -> Result<Self, NTSTATUS> {
        Self::try_new(init, attributes)
    }

    /// Perform an operation with the waitlock acquired.
    ///
    /// # Performance
    ///
    /// Note that this function takes a closure, so you may think that this is
    /// not as performant as manually acquiring and releasing the lock due to
    /// the dynamic dispatch involved with the closure. But you are wrong :)
    ///
    /// The compiler will generate a copy of this function for the closure
    /// you have written and in most cases will inline the code into the calling
    /// function. This often means that the generated code is as slim and fast
    /// as the code you would've written manually.
    ///
    /// # IRQL
    ///
    /// If `timeout` is non-zero or `None`, this function _MUST_ be called at
    /// `IRQL = PASSIVE_LEVEL`.
    /// If `timeout` is `Some(0)`, this function must be called at
    /// `IRQL < DISPATCH_LEVEL`.
    pub fn with<R>(
        &self,
        timeout: Option<i64>,
        f: impl FnOnce(&mut T) -> R,
    ) -> Result<R, NTSTATUS> {
        // SAFETY: `wdf_wait_lock` is a private member of `WaitLock`, originally created
        // by WDF, and this module guarantees that it is always in a valid state.
        let s = unsafe {
            macros::call_unsafe_wdf_function_binding! {
                WdfWaitLockAcquire(self.wdf_wait_lock, timeout.as_ref().map(|t| t as *const _ as *mut i64).unwrap_or(core::ptr::null_mut()))
            }
        };
        if !nt_success(s) {
            return Err(s);
        }

        // SAFETY: Because we hold the lock, we know that no one else is concurrently
        // accessing `self.inner`.
        let r = f(unsafe { &mut *self.inner.get() });

        // SAFETY: `wdf_wait_lock` is a private member of `WaitLock`, originally created
        // by WDF, and this module guarantees that it is always in a valid state.
        let _ = unsafe {
            macros::call_unsafe_wdf_function_binding! {
                WdfWaitLockRelease(self.wdf_wait_lock)
            }
        };

        Ok(r)
    }
}
