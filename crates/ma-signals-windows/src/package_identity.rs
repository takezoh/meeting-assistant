//! Package identity of a process (contract-process-package-identity, discretion-package-identity-probe).
//!
//! `Subject::Process.package_family_name` is `Option<String>` where `None` means "not packaged"
//! and never "unknown". The probe therefore returns a three-way result and the collector maps
//! [`PackageIdentity::QueryFailed`] to `None` as well, counting it in its diagnostics so a
//! transient failure stays distinguishable from a classic Win32 executable without widening the
//! closed envelope.

/// Result of asking the OS for a process's package identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageIdentity {
    /// The process runs with package identity; the value is its package family name.
    Packaged(String),
    /// The process is a classic executable without package identity
    /// (`APPMODEL_ERROR_NO_PACKAGE` on Windows).
    NotPackaged,
    /// The query itself failed (access denied, the process exited mid-query, an unexpected
    /// error). The envelope reports `None`; the collector records the failure.
    QueryFailed { code: u32 },
}

impl PackageIdentity {
    /// The envelope value: only a confirmed package family name is carried.
    pub fn family_name(&self) -> Option<String> {
        match self {
            PackageIdentity::Packaged(name) => Some(name.clone()),
            PackageIdentity::NotPackaged | PackageIdentity::QueryFailed { .. } => None,
        }
    }
}

/// Probes one process's package identity. The live implementation is [`WindowsPackageIdentityProbe`];
/// fakes return scripted results.
pub trait PackageIdentityProbe {
    fn probe(&mut self, pid: u32) -> PackageIdentity;
}

/// `GetPackageFamilyName` over a process handle opened with `PROCESS_QUERY_LIMITED_INFORMATION`.
///
/// Discretion `discretion-package-identity-probe`: `APPMODEL_ERROR_NO_PACKAGE` (15700) is the only
/// code that means "not packaged"; every other failure is a `QueryFailed` so that `None` in the
/// envelope is never produced from an inconclusive probe without a diagnostic count.
#[cfg(windows)]
#[derive(Debug, Default)]
pub struct WindowsPackageIdentityProbe;

#[cfg(windows)]
impl PackageIdentityProbe for WindowsPackageIdentityProbe {
    fn probe(&mut self, pid: u32) -> PackageIdentity {
        use windows::core::PWSTR;
        use windows::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, WIN32_ERROR};
        use windows::Win32::Storage::Packaging::Appx::GetPackageFamilyName;
        use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

        const APPMODEL_ERROR_NO_PACKAGE: u32 = 15700;

        // SAFETY: OpenProcess with a limited-information right; the handle is closed below.
        let handle = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
            Ok(h) => h,
            Err(e) => {
                return PackageIdentity::QueryFailed {
                    code: e.code().0 as u32,
                }
            }
        };
        let mut len: u32 = 0;
        // SAFETY: a null buffer with len 0 asks for the required length.
        let first = unsafe { GetPackageFamilyName(handle, &mut len, None) };
        let result = if first == WIN32_ERROR(APPMODEL_ERROR_NO_PACKAGE) {
            PackageIdentity::NotPackaged
        } else if first.0 == 0 || first.0 == 122 {
            // ERROR_INSUFFICIENT_BUFFER (122) with the required length, or an empty name.
            let mut buf = vec![0u16; len.max(1) as usize];
            // SAFETY: buffer of the length the first call reported.
            let second =
                unsafe { GetPackageFamilyName(handle, &mut len, Some(PWSTR(buf.as_mut_ptr()))) };
            if second == ERROR_SUCCESS {
                let end = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
                PackageIdentity::Packaged(String::from_utf16_lossy(&buf[..end]))
            } else if second == WIN32_ERROR(APPMODEL_ERROR_NO_PACKAGE) {
                PackageIdentity::NotPackaged
            } else {
                PackageIdentity::QueryFailed { code: second.0 }
            }
        } else {
            PackageIdentity::QueryFailed { code: first.0 }
        };
        // SAFETY: handle came from OpenProcess above.
        let _ = unsafe { CloseHandle(handle) };
        result
    }
}

/// Scripted probe results keyed by pid; unknown pids are reported as not packaged.
#[derive(Debug, Default, Clone)]
pub struct FakePackageIdentityProbe {
    results: std::collections::BTreeMap<u32, PackageIdentity>,
}

impl FakePackageIdentityProbe {
    pub fn with(mut self, pid: u32, identity: PackageIdentity) -> Self {
        self.results.insert(pid, identity);
        self
    }
}

impl PackageIdentityProbe for FakePackageIdentityProbe {
    fn probe(&mut self, pid: u32) -> PackageIdentity {
        self.results
            .get(&pid)
            .cloned()
            .unwrap_or(PackageIdentity::NotPackaged)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_confirmed_package_reaches_the_envelope() {
        assert_eq!(
            PackageIdentity::Packaged("Publisher.App_abc".into()).family_name(),
            Some("Publisher.App_abc".to_string())
        );
        assert_eq!(PackageIdentity::NotPackaged.family_name(), None);
        assert_eq!(PackageIdentity::QueryFailed { code: 5 }.family_name(), None);
    }
}
