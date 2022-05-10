use std::fmt::{self, Display};
use std::rc::Rc;

use derive_more::Display;

use crate::StructDecl;

#[derive(Clone, Copy, Debug, Display, Hash, PartialEq, Eq)]
pub enum ScalarType {
    #[display(fmt = "bool")]
    Bool,
    #[display(fmt = "i32")]
    I32,
    #[display(fmt = "u32")]
    U32,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum DataType {
    Scalar(ScalarType),
    Vector(u8, ScalarType),
    Array(Rc<DataType>, Option<u32>),
    Struct(Rc<StructDecl>),
}

impl DataType {
    pub fn map(&self, scalar: ScalarType) -> DataType {
        match self {
            DataType::Scalar(_) => DataType::Scalar(scalar),
            DataType::Vector(n, _) => DataType::Vector(*n, scalar),
            DataType::Array(_, _) => unimplemented!(),
            DataType::Struct(_) => unimplemented!(),
        }
    }
}

impl Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataType::Scalar(t) => write!(f, "{}", t),
            DataType::Vector(n, t) => write!(f, "vec{}<{}>", n, t),
            DataType::Array(inner, n) => {
                write!(f, "array<{inner}")?;
                if let Some(n) = n {
                    write!(f, ", {n}")?;
                }
                write!(f, ">")
            }
            DataType::Struct(decl) => write!(f, "{}", decl.name),
        }
    }
}

impl From<ScalarType> for DataType {
    fn from(scalar: ScalarType) -> Self {
        DataType::Scalar(scalar)
    }
}
