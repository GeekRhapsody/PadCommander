#[derive(Clone, Debug, Default)]
pub struct LocalFileSystem;

pub trait VirtualFileSystem {
    fn scheme(&self) -> &'static str;
}

impl VirtualFileSystem for LocalFileSystem {
    fn scheme(&self) -> &'static str {
        "local"
    }
}

pub fn scheme_name(vfs: &dyn VirtualFileSystem) -> &'static str {
    vfs.scheme()
}
