//! Security descriptor construction (contract-ipc-transport-authz): a pipe or file grants the
//! owning user only. The descriptor is built as data and rendered to SDDL; applying it to a
//! handle is the platform unit's job.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessMask {
    /// `GENERIC_ALL`.
    FullControl,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ace {
    pub allow: bool,
    pub sid: String,
    pub access: AccessMask,
}

/// The well-known SIDs that must never appear in a descriptor this product creates.
pub const FORBIDDEN_SIDS: [&str; 5] = ["WD", "S-1-1-0", "AU", "NU", "AN"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityDescriptor {
    pub owner: String,
    pub dacl: Vec<Ace>,
}

impl SecurityDescriptor {
    /// A descriptor granting full control to `owner_sid` and nothing to anyone else.
    pub fn owner_only(owner_sid: &str) -> SecurityDescriptor {
        assert!(
            !FORBIDDEN_SIDS.contains(&owner_sid),
            "a well-known group is never an owner"
        );
        SecurityDescriptor {
            owner: owner_sid.to_string(),
            dacl: vec![Ace {
                allow: true,
                sid: owner_sid.to_string(),
                access: AccessMask::FullControl,
            }],
        }
    }

    /// Render as SDDL (`O:<sid>D:(A;;GA;;;<sid>)`).
    pub fn to_sddl(&self) -> String {
        let aces: String = self
            .dacl
            .iter()
            .map(|a| {
                format!(
                    "({};;{};;;{})",
                    if a.allow { "A" } else { "D" },
                    match a.access {
                        AccessMask::FullControl => "GA",
                    },
                    a.sid
                )
            })
            .collect();
        format!("O:{}D:{}", self.owner, aces)
    }

    /// True when every ACE grants the owner and no forbidden principal appears.
    pub fn grants_owner_only(&self) -> bool {
        !self.dacl.is_empty()
            && self.dacl.iter().all(|a| {
                a.allow && a.sid == self.owner && !FORBIDDEN_SIDS.contains(&a.sid.as_str())
            })
    }
}

/// Creation parameters for the engine's control pipe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipeSecurity {
    pub name: String,
    pub descriptor: SecurityDescriptor,
    /// `FILE_FLAG_FIRST_PIPE_INSTANCE`: the name cannot be pre-squatted.
    pub first_pipe_instance: bool,
}

impl PipeSecurity {
    pub fn engine_pipe(installation_id: &str, owner_sid: &str) -> PipeSecurity {
        PipeSecurity {
            name: format!("\\\\.\\pipe\\MeetingAssistant.engine.{installation_id}"),
            descriptor: SecurityDescriptor::owner_only(owner_sid),
            first_pipe_instance: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acl_pipe_descriptor_owner_only() {
        let owner = "S-1-5-21-1111111111-2222222222-3333333333-1001";
        let pipe = PipeSecurity::engine_pipe("inst-42", owner);
        assert!(pipe.first_pipe_instance);
        assert_eq!(pipe.name, "\\\\.\\pipe\\MeetingAssistant.engine.inst-42");
        let d = &pipe.descriptor;
        assert_eq!(d.owner, owner);
        assert_eq!(
            d.dacl,
            vec![Ace {
                allow: true,
                sid: owner.into(),
                access: AccessMask::FullControl
            }],
            "exactly one ACE: the owner"
        );
        assert!(d.grants_owner_only());
        let sddl = d.to_sddl();
        assert_eq!(sddl, format!("O:{owner}D:(A;;GA;;;{owner})"));
        for principal in FORBIDDEN_SIDS {
            assert!(
                !sddl.contains(&format!(";;;{principal})")),
                "no ACE for {principal}"
            );
        }
        let mut permissive = d.clone();
        permissive.dacl.push(Ace {
            allow: true,
            sid: "WD".into(),
            access: AccessMask::FullControl,
        });
        assert!(!permissive.grants_owner_only());
    }
}
