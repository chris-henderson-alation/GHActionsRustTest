use super::import::Import;
use super::namespace::Namespace;

pub struct WorkFlow {}

/// A `WorkFlow` takes ownership of a [Namespace](Namespace). At each stage of a `WorkFlow`,
/// ownership of this namespace is passed onto the next step. In this way, we can guarantee
/// that the namespace will exist for the complete duration of the installation procedure
/// while also guaranteeing that the namespace is ultimately cleaned up in all exit scenarios.
impl WorkFlow {
    pub fn new_workflow(namespace: &Namespace) -> Import {
        Import { namespace }
    }
}
