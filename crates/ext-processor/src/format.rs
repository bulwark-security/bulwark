#[derive(Clone, Copy, Debug)]
pub(crate) struct Float3Formatter(pub f64);

impl std::fmt::Display for Float3Formatter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.3}", self.0)?;

        Ok(())
    }
}
