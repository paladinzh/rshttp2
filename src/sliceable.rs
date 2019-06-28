pub trait Sliceable<T> {
    fn as_slice(&self) -> &[T];
}

impl<T> Sliceable<T> for Vec<T> {
    fn as_slice(&self) -> &[T] {
        self.as_slice()
    }
}


pub struct AnySliceable<T>(Box<dyn Sliceable<T>>);

impl<T> AnySliceable<T> {
    pub fn new(obj: impl Sliceable<T> + 'static) -> AnySliceable<T> {
        AnySliceable(Box::new(obj))
    }
}

impl<T> Sliceable<T> for AnySliceable<T> {
    fn as_slice(&self) -> &[T] {
        self.0.as_slice()
    }
}