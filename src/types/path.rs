//! Path traversal tracking for nested HCL values and error diagnostics (`cty.Path` equivalent).

use crate::types::val::Value;
use std::fmt;

/// An individual step along a nested value path.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PathStep {
    /// Attribute lookup step (`.attribute`).
    GetAttr(String),
    /// Numeric index step (`[0]`).
    Index(Value),
    /// Map key index step (`["key"]`).
    Key(Value),
}

impl fmt::Display for PathStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GetAttr(attr) => write!(f, ".{attr}"),
            Self::Index(idx) => write!(f, "[{idx}]"),
            Self::Key(key) => write!(f, "[{key}]"),
        }
    }
}

/// A sequence of steps representing a path through nested data structures.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Path {
    steps: Vec<PathStep>,
}

impl Path {
    /// Creates an empty path pointing to the root value.
    #[must_use]
    pub fn empty() -> Self {
        Self { steps: Vec::new() }
    }

    /// Creates a new path from a sequence of steps.
    ///
    /// # Arguments
    /// * `steps` - The sequence of path steps.
    #[must_use]
    pub fn new(steps: Vec<PathStep>) -> Self {
        Self { steps }
    }

    /// Appends a new step to this path, returning the new extended path.
    ///
    /// # Arguments
    /// * `step` - The step to append.
    #[must_use]
    pub fn with_step(&self, step: PathStep) -> Self {
        let mut steps = self.steps.clone();
        steps.push(step);
        Self { steps }
    }

    /// Appends a new step in-place.
    ///
    /// # Arguments
    /// * `step` - The step to append.
    pub fn push(&mut self, step: PathStep) {
        self.steps.push(step);
    }

    /// Removes and returns the last step in-place, if any.
    pub fn pop(&mut self) -> Option<PathStep> {
        self.steps.pop()
    }

    /// Returns the parent path, or `None` if this path is empty.
    #[must_use]
    pub fn parent(&self) -> Option<Self> {
        if self.steps.is_empty() {
            None
        } else {
            let mut steps = self.steps.clone();
            steps.pop();
            Some(Self { steps })
        }
    }

    /// Returns a slice of the steps in this path.
    #[must_use]
    pub fn steps(&self) -> &[PathStep] {
        &self.steps
    }

    /// Returns `true` if this path is empty (points to root).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Returns the number of steps in this path.
    #[must_use]
    pub fn len(&self) -> usize {
        self.steps.len()
    }
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.steps.is_empty() {
            write!(f, "(root)")
        } else {
            for (idx, step) in self.steps.iter().enumerate() {
                match step {
                    PathStep::GetAttr(attr) => {
                        if idx == 0 {
                            write!(f, "{attr}")?;
                        } else {
                            write!(f, ".{attr}")?;
                        }
                    }
                    PathStep::Index(val) | PathStep::Key(val) => write!(f, "[{val}]")?,
                }
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode::EncodeValue;

    #[test]
    fn test_path_and_steps() {
        let p0 = Path::empty();
        assert!(p0.is_empty());
        assert_eq!(p0.len(), 0);
        assert_eq!(p0.to_string(), "(root)");
        assert_eq!(p0.parent(), None);

        let p1 = p0.with_step(PathStep::GetAttr("server".to_string()));
        assert_eq!(p1.to_string(), "server");
        assert_eq!(p1.len(), 1);

        let p2 = p1.with_step(PathStep::Index(0_i64.encode_value()));
        assert_eq!(p2.to_string(), "server[0]");

        let p3 = p2.with_step(PathStep::GetAttr("ip".to_string()));
        assert_eq!(p3.to_string(), "server[0].ip");

        let p4 = p3.with_step(PathStep::Key("v4".encode_value()));
        assert_eq!(p4.to_string(), "server[0].ip[\"v4\"]");

        let parent = p4.parent().expect("parent");
        assert_eq!(parent.to_string(), "server[0].ip");

        // Test push & pop
        let mut p_mut = Path::new(vec![PathStep::GetAttr("foo".to_string())]);
        p_mut.push(PathStep::Index(1_i64.encode_value()));
        assert_eq!(p_mut.steps().len(), 2);
        let popped = p_mut.pop();
        assert_eq!(popped, Some(PathStep::Index(1_i64.encode_value())));
        assert_eq!(p_mut.len(), 1);

        // Test Display of individual steps
        assert_eq!(PathStep::GetAttr("sub".to_string()).to_string(), ".sub");
        assert_eq!(PathStep::Index(5_i64.encode_value()).to_string(), "[5]");
        assert_eq!(PathStep::Key("k".encode_value()).to_string(), "[\"k\"]");
    }

    /// Writer that fails after a specified number of successful writes.
    struct CountedWriter(usize);

    impl fmt::Write for CountedWriter {
        fn write_str(&mut self, _s: &str) -> fmt::Result {
            if self.0 == 0 {
                Err(fmt::Error)
            } else {
                self.0 -= 1;
                Ok(())
            }
        }
    }

    #[test]
    fn test_path_display_fmt_errors() {
        use std::fmt::Write;

        // 1. GetAttr at idx 0 fails
        let p_attr0 = Path::new(vec![PathStep::GetAttr("a".to_string())]);
        let mut w0 = CountedWriter(0);
        assert!(write!(&mut w0, "{p_attr0}").is_err());

        // 2. GetAttr at idx > 0 fails
        let p_attr1 = Path::new(vec![
            PathStep::GetAttr("a".to_string()),
            PathStep::GetAttr("b".to_string()),
        ]);
        let mut w1 = CountedWriter(1);
        assert!(write!(&mut w1, "{p_attr1}").is_err());

        // 3. Index/Key fails
        let p_idx = Path::new(vec![PathStep::Index(0_i64.encode_value())]);
        let mut w_idx = CountedWriter(0);
        assert!(write!(&mut w_idx, "{p_idx}").is_err());
    }
}
