use crate::prelude::*;

pub trait Argument {
    fn apply<'a, C: IsCommandWrapper + ?Sized>(&self, c: &'a mut C) -> &'a mut C;
}
