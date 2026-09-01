//! Built-in meshes (ENG-161).
//!
//! `MeshRenderer.mesh` has carried three incompatible conventions at once: the empty string
//! (the Rust scaffold's "use the built-in"), a bare primitive name like `"cube"` (what the
//! old TypeScript wrote), and `asset:<ulid>` (what the schema validator actually demands).
//! The viewport guessed between them by sniffing the string, which is why a `.glb` rendered
//! as a grey box and a scaffolded cube rendered as something else again.
//!
//! One vocabulary instead: a built-in is `builtin:<name>` from the fixed list below, an
//! imported mesh is `asset:<ulid>`, and the empty string means *unset* — draw the placeholder
//! and say so. The schema accepts exactly those three forms and nothing else.

use serde::{Deserialize, Serialize};
use specta::Type;

/// A primitive the renderer can build without an asset file.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinMesh {
    Cube,
    Sphere,
    Plane,
    Capsule,
    Cylinder,
    Cone,
    Quad,
    Torus,
}

impl BuiltinMesh {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cube => "cube",
            Self::Sphere => "sphere",
            Self::Plane => "plane",
            Self::Capsule => "capsule",
            Self::Cylinder => "cylinder",
            Self::Cone => "cone",
            Self::Quad => "quad",
            Self::Torus => "torus",
        }
    }

    /// The canonical reference written into a `MeshRenderer`.
    #[must_use]
    pub fn reference(self) -> String {
        format!("{BUILTIN_PREFIX}{}", self.as_str())
    }

    #[must_use]
    pub fn all() -> Vec<Self> {
        vec![
            Self::Cube,
            Self::Sphere,
            Self::Plane,
            Self::Capsule,
            Self::Cylinder,
            Self::Cone,
            Self::Quad,
            Self::Torus,
        ]
    }
}

pub const BUILTIN_PREFIX: &str = "builtin:";

/// Parse a `builtin:` reference. `None` for anything else, including a bare `"cube"` — a
/// bare name is the old ambiguous form and is not silently accepted.
#[must_use]
pub fn builtin_from_reference(reference: &str) -> Option<BuiltinMesh> {
    let name = reference.strip_prefix(BUILTIN_PREFIX)?;
    BuiltinMesh::all()
        .into_iter()
        .find(|mesh| mesh.as_str() == name)
}

/// Every built-in reference, for the schema hint and the mesh picker.
#[must_use]
pub fn builtin_references() -> Vec<String> {
    BuiltinMesh::all()
        .into_iter()
        .map(BuiltinMesh::reference)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{builtin_from_reference, builtin_references, BuiltinMesh};

    #[test]
    fn references_round_trip_and_bare_names_are_not_accepted() {
        for mesh in BuiltinMesh::all() {
            let reference = mesh.reference();
            assert_eq!(builtin_from_reference(&reference), Some(mesh));
        }
        // The old ambiguous form must not resolve: it is exactly the guessing this replaces.
        assert_eq!(builtin_from_reference("cube"), None);
        assert_eq!(builtin_from_reference("builtin:hologram"), None);
        assert_eq!(builtin_from_reference(""), None);
    }

    #[test]
    fn every_builtin_is_listed_once() {
        let references = builtin_references();
        assert_eq!(references.len(), BuiltinMesh::all().len());
        let mut sorted = references.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), references.len());
        assert!(references.contains(&"builtin:cube".to_owned()));
    }
}
