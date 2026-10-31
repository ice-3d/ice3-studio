use std::collections::BTreeMap;

#[derive(Debug, Default)]
pub struct ResourceReferences {
    geometries: BTreeMap<String, (String, String)>,
    topology_refs: BTreeMap<String, usize>,
    attribute_refs: BTreeMap<String, usize>,
}

impl ResourceReferences {
    pub fn add_geometry(&mut self, geometry: &str, topology: &str, attribute: &str) {
        if self.geometries.contains_key(geometry) {
            return;
        }
        self.geometries.insert(
            geometry.to_owned(),
            (topology.to_owned(), attribute.to_owned()),
        );
        *self.topology_refs.entry(topology.to_owned()).or_default() += 1;
        *self.attribute_refs.entry(attribute.to_owned()).or_default() += 1;
    }

    pub fn remove_geometry(&mut self, geometry: &str) {
        let Some((topology, attribute)) = self.geometries.remove(geometry) else {
            return;
        };
        decrement(&mut self.topology_refs, &topology);
        decrement(&mut self.attribute_refs, &attribute);
    }

    pub fn topology_is_referenced(&self, topology: &str) -> bool {
        self.topology_refs
            .get(topology)
            .is_some_and(|count| *count > 0)
    }

    pub fn evictable_attributes(&self) -> impl Iterator<Item = &str> {
        self.attribute_refs
            .iter()
            .filter(|(_, count)| **count == 0)
            .map(|(id, _)| id.as_str())
    }

    pub fn evictable_topologies(&self) -> impl Iterator<Item = &str> {
        self.topology_refs
            .iter()
            .filter(|(_, count)| **count == 0)
            .map(|(id, _)| id.as_str())
    }

    pub fn geometry_ids(&self) -> impl Iterator<Item = &str> {
        self.geometries.keys().map(String::as_str)
    }
}

fn decrement(references: &mut BTreeMap<String, usize>, id: &str) {
    if let Some(count) = references.get_mut(id) {
        *count = count.saturating_sub(1);
    }
}
