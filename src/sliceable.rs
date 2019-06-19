pub trait Sliceable<T> {
    fn as_slice(&self) -> &[T];
}

impl<T> Sliceable<T> for Vec<T> {
    fn as_slice(&self) -> &[T] {
        self.as_slice()
    }
}
