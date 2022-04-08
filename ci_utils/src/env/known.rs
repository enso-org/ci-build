use super::*;
use crate::env::new::PathLike;

pub const PATH: PathLike = PathLike("PATH");


#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::new::TypedVariable;

    #[test]
    fn foo() {
        dbg!(PATH.get());
    }
}
