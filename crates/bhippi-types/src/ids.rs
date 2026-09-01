use serde::{Deserialize, Serialize};
use specta::Type;
use std::fmt;
use std::str::FromStr;
use ulid::Ulid;

macro_rules! define_id {
    ($name:ident) => {
        #[derive(
            Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Type,
        )]
        #[serde(transparent)]
        pub struct $name(Ulid);

        impl $name {
            #[must_use]
            pub fn new() -> Self {
                Self(Ulid::new())
            }

            #[must_use]
            pub const fn from_ulid(value: Ulid) -> Self {
                Self(value)
            }

            #[must_use]
            pub const fn into_ulid(self) -> Ulid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl From<Ulid> for $name {
            fn from(value: Ulid) -> Self {
                Self::from_ulid(value)
            }
        }

        impl From<$name> for Ulid {
            fn from(value: $name) -> Self {
                value.into_ulid()
            }
        }

        impl FromStr for $name {
            type Err = ulid::DecodeError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Ulid::from_str(value).map(Self::from_ulid)
            }
        }
    };
}

define_id!(SessionId);
define_id!(NodeId);
define_id!(DotId);
define_id!(SourceId);
define_id!(ImageId);
define_id!(PostId);
define_id!(SkillId);
define_id!(ProviderId);
define_id!(TickerEventId);
define_id!(GameId);
define_id!(SceneId);
define_id!(EntityId);
define_id!(AssetId);
define_id!(BuildId);
define_id!(TransactionId);
define_id!(ProjectId);
define_id!(ModuleId);
define_id!(FileId);
define_id!(SymbolId);

#[cfg(test)]
mod tests {
    use super::SessionId;
    use std::str::FromStr;

    #[test]
    fn id_text_round_trip_preserves_the_ulid() {
        let original = SessionId::new();
        let parsed = SessionId::from_str(&original.to_string());

        assert_eq!(parsed, Ok(original));
    }
}
