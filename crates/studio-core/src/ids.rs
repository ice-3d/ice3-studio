use std::fmt::{Display, Formatter, Result as FmtResult};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum IdError {
    #[error("ID prefix must contain only lowercase ASCII letters, digits, or hyphens")]
    InvalidPrefix,
}

macro_rules! define_id {
    ($name:ident) => {
        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            PartialOrd,
            Ord,
            Hash,
            Serialize,
            Deserialize,
            JsonSchema,
            TS,
        )]
        #[serde(transparent)]
        #[ts(export)]
        pub struct $name(String);

        impl $name {
            pub fn new(prefix: &str, ordinal: u64) -> Result<Self, IdError> {
                if prefix.is_empty()
                    || !prefix.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
                    })
                {
                    return Err(IdError::InvalidPrefix);
                }
                Ok(Self(format!("{prefix}-{ordinal:06}")))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn is_valid(&self) -> bool {
                let Some((prefix, ordinal)) = self.0.rsplit_once('-') else {
                    return false;
                };
                !prefix.is_empty()
                    && ordinal.len() == 6
                    && ordinal.bytes().all(|byte| byte.is_ascii_digit())
                    && prefix.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
                    })
            }
        }

        impl Display for $name {
            fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
                formatter.write_str(&self.0)
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_owned())
            }
        }
    };
}

define_id!(NodeId);
define_id!(MaterialId);
define_id!(RelationId);
define_id!(ConstraintId);

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(transparent)]
#[ts(export)]
pub struct Revision(u64);

impl Revision {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn as_u64(self) -> u64 {
        self.0
    }

    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }
}
