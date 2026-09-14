//! Visibility model: ModulePrivate, PackageInternal, Public.

use crate::{ModuleId, PackageInstanceId};
use serde::Serialize;

/// Visibility of a symbol.
///
/// The matrix:
/// | From              | ModulePrivate | PackageInternal | Public |
/// |-------------------|:---:|:---:|:---:|
/// | Same module       | yes | yes | yes |
/// | Same pkg, diff mod| no  | yes | yes |
/// | Other package     | no  | no  | yes |
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Default)]
pub enum Visibility {
    /// Visible only within the declaring module (default).
    #[default]
    ModulePrivate,
    /// Visible within the same package (declared with `export`).
    PackageInternal,
    /// Visible externally (declared with `export` + in manifest publicModules).
    Public,
}

impl Visibility {
    pub fn as_str(self) -> &'static str {
        match self {
            Visibility::ModulePrivate => "module_private",
            Visibility::PackageInternal => "package_internal",
            Visibility::Public => "public",
        }
    }

    /// Can a module in `accessor_package` see a symbol with this visibility,
    /// declared in `decl_module` which belongs to `decl_package`?
    pub fn is_accessible_from(
        self,
        decl_module: ModuleId,
        decl_package: PackageInstanceId,
        accessor_module: ModuleId,
        accessor_package: PackageInstanceId,
    ) -> bool {
        match self {
            Visibility::ModulePrivate => decl_module == accessor_module,
            Visibility::PackageInternal => decl_package == accessor_package,
            Visibility::Public => true,
        }
    }
}
