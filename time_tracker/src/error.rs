use std::{
  fmt::{
    Display,
    Formatter,
    Result
  },
  error::Error
};

#[macro_export]
macro_rules! box_err {
  ($ expr:expr) => { Result::Err(Box::new($expr)) };
}

#[derive(Debug)]
pub struct AddError(pub String);

impl Display for AddError {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "Cannot add process: {}", self.0)
    }
}

impl Error for AddError {}
