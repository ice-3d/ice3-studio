use std::collections::{BTreeMap, BTreeSet};

use ifc_lite_processing::data_model::{
    DataModel, EntityMetadata, Property, QuantitySet, Relationship, SpatialNode,
};
use serde::Serialize;
use serde_json::Value;

use crate::{ImportError, ImportWarning, NativeImportLimits};

const MAX_METADATA_ARRAY_LENGTH: usize = 256;
const ESTIMATED_BYTES_PER_IFC_ENTITY: usize = 2_048;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IfcMetadata {
    pub schema: String,
    pub project_id: u32,
    pub nodes: BTreeMap<u32, IfcNodeMetadata>,
    pub products: BTreeMap<u32, IfcProductMetadata>,
    pub systems: BTreeMap<u32, IfcSystemMetadata>,
    pub warnings: Vec<ImportWarning>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IfcNodeMetadata {
    pub express_id: u32,
    pub parent_id: Option<u32>,
    pub path: String,
    pub ifc_type: String,
    pub name: String,
    pub children: Vec<u32>,
    pub elements: Vec<u32>,
    pub elevation: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IfcProductMetadata {
    pub express_id: u32,
    pub ifc_type: String,
    pub global_id: Option<String>,
    pub selection_id: String,
    pub name: String,
    pub description: Option<String>,
    pub object_type: Option<String>,
    pub predefined_type: Option<String>,
    pub tag: Option<String>,
    pub property_sets: Vec<IfcPropertySetMetadata>,
    pub quantity_sets: Vec<IfcQuantitySetMetadata>,
    pub systems: Vec<IfcSystemReference>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IfcPropertySetMetadata {
    pub express_id: u32,
    pub name: String,
    pub properties: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IfcQuantitySetMetadata {
    pub express_id: u32,
    pub name: String,
    pub quantities: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IfcSystemReference {
    pub express_id: u32,
    pub global_id: Option<String>,
    pub name: String,
    pub ifc_type: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IfcSystemMetadata {
    pub reference: IfcSystemReference,
    pub members: Vec<String>,
}

pub fn extract_ifc_metadata(
    bytes: &[u8],
    asset_id: &str,
    limits: NativeImportLimits,
) -> Result<IfcMetadata, ImportError> {
    let asset_id = asset_id.trim();
    if asset_id.is_empty() {
        return Err(ImportError::InvalidSource(
            "asset ID must not be empty".into(),
        ));
    }
    if bytes.len() > limits.max_memory_bytes {
        return Err(ImportError::LimitExceeded(format!(
            "IFC input has {} bytes; limit is {}",
            bytes.len(),
            limits.max_memory_bytes
        )));
    }
    let schema = ifc_lite_processing::detect_schema_version(bytes);
    if schema != "IFC4X3" {
        return Err(ImportError::SourceDiagnostic {
            code: "ICE3_STUDIO_IMPORT_IFC_SCHEMA_UNSUPPORTED".into(),
            message: format!("unsupported IFC schema: {schema}"),
        });
    }
    let entity_count = preflight_ifc_entities(bytes, limits)?;
    let data = ifc_lite_processing::data_model::extract_data_model(bytes);
    if data.entities.len() != entity_count {
        return Err(ImportError::LimitExceeded(format!(
            "IFC entity count changed during extraction: scanned {}, decoded {}",
            entity_count,
            data.entities.len(),
        )));
    }
    build_metadata(data, schema, asset_id)
}

fn preflight_ifc_entities(bytes: &[u8], limits: NativeImportLimits) -> Result<usize, ImportError> {
    let mut scanner = ifc_lite_core::EntityScanner::new(bytes);
    let mut entity_count = 0_usize;
    while scanner.next_entity().is_some() {
        entity_count = entity_count
            .checked_add(1)
            .ok_or_else(|| ImportError::LimitExceeded("IFC entity count overflowed".into()))?;
        if entity_count > limits.max_nodes {
            return Err(ImportError::LimitExceeded(format!(
                "IFC has more than {} entities",
                limits.max_nodes
            )));
        }
    }
    let estimated_bytes = bytes
        .len()
        .checked_add(
            entity_count
                .checked_mul(ESTIMATED_BYTES_PER_IFC_ENTITY)
                .ok_or_else(|| {
                    ImportError::LimitExceeded("IFC decoded-memory estimate overflowed".into())
                })?,
        )
        .ok_or_else(|| {
            ImportError::LimitExceeded("IFC decoded-memory estimate overflowed".into())
        })?;
    if estimated_bytes > limits.max_memory_bytes {
        return Err(ImportError::LimitExceeded(format!(
            "IFC decoded memory is approximately {estimated_bytes} bytes; limit is {}",
            limits.max_memory_bytes
        )));
    }
    Ok(entity_count)
}

fn build_metadata(
    data: DataModel,
    schema: &str,
    asset_id: &str,
) -> Result<IfcMetadata, ImportError> {
    let project_id = data.spatial_hierarchy.project_id;
    if project_id == 0 {
        return Err(ImportError::InvalidSource(
            "IFC spatial hierarchy has no project".into(),
        ));
    }

    let mut warnings = BTreeSet::new();
    let nodes = build_nodes(&data.spatial_hierarchy.nodes)?;
    let product_ids = collect_product_ids(&data, &nodes);
    let entities = data
        .entities
        .into_iter()
        .map(|entity| (entity.entity_id, entity))
        .collect::<BTreeMap<_, _>>();
    let property_sets = data
        .property_sets
        .iter()
        .map(|set| (set.pset_id, set))
        .collect::<BTreeMap<_, _>>();
    let quantity_sets = data
        .quantity_sets
        .iter()
        .map(|set| (set.qset_id, set))
        .collect::<BTreeMap<_, _>>();
    let products_by_id = data
        .relationships
        .iter()
        .filter(|relation| is_property_relation(&relation.rel_type))
        .filter_map(|relation| {
            property_sets
                .contains_key(&relation.relating_id)
                .then_some(())
                .or_else(|| {
                    quantity_sets
                        .contains_key(&relation.relating_id)
                        .then_some(())
                })
                .map(|_| (relation.related_id, relation.relating_id))
        })
        .fold(
            BTreeMap::<u32, BTreeSet<u32>>::new(),
            |mut definitions, (product, definition)| {
                definitions.entry(product).or_default().insert(definition);
                definitions
            },
        );
    let mut used_global_ids = BTreeSet::new();
    let selection_ids = build_selection_ids(
        &product_ids,
        &entities,
        asset_id,
        &mut used_global_ids,
        &mut warnings,
    )?;
    let system_members = build_system_members(&data.relationships, &entities);
    let systems = build_systems(&system_members, &selection_ids, &entities);
    let mut products = BTreeMap::new();
    for express_id in product_ids {
        let entity = entities.get(&express_id).ok_or_else(|| {
            ImportError::InvalidSource(format!("IFC product #{express_id} is missing"))
        })?;
        let product = build_product(
            entity,
            &products_by_id,
            &property_sets,
            &quantity_sets,
            &selection_ids,
            &systems,
            &mut warnings,
        )?;
        products.insert(express_id, product);
    }

    Ok(IfcMetadata {
        schema: schema.into(),
        project_id,
        nodes,
        products,
        systems,
        warnings: warnings.into_iter().collect(),
    })
}

fn build_nodes(nodes: &[SpatialNode]) -> Result<BTreeMap<u32, IfcNodeMetadata>, ImportError> {
    let mut result = BTreeMap::new();
    for node in nodes {
        if result
            .insert(
                node.entity_id,
                IfcNodeMetadata {
                    express_id: node.entity_id,
                    parent_id: (node.parent_id != 0).then_some(node.parent_id),
                    path: node.path.clone(),
                    ifc_type: node.type_name.to_ascii_uppercase(),
                    name: node
                        .name
                        .clone()
                        .filter(|name| !name.is_empty())
                        .unwrap_or_else(|| format!("ifc/{}/{}", node.type_name, node.entity_id)),
                    children: sorted_ids(&node.children_ids),
                    elements: sorted_ids(&node.element_ids),
                    elevation: node.elevation.filter(|value| value.is_finite()),
                },
            )
            .is_some()
        {
            return Err(ImportError::InvalidSource(format!(
                "duplicate IFC spatial node #{}",
                node.entity_id
            )));
        }
    }
    Ok(result)
}

fn collect_product_ids(data: &DataModel, nodes: &BTreeMap<u32, IfcNodeMetadata>) -> BTreeSet<u32> {
    let mut ids = BTreeSet::new();
    for node in nodes.values() {
        ids.extend(node.elements.iter().copied());
    }
    for entity in &data.entities {
        if entity.has_geometry && !nodes.contains_key(&entity.entity_id) {
            ids.insert(entity.entity_id);
        }
    }
    for relation in &data.relationships {
        if is_property_relation(&relation.rel_type) || is_group_relation(&relation.rel_type) {
            ids.insert(relation.related_id);
        }
    }
    ids
}

fn build_product(
    entity: &EntityMetadata,
    products_by_id: &BTreeMap<u32, BTreeSet<u32>>,
    all_property_sets: &BTreeMap<u32, &ifc_lite_processing::data_model::PropertySet>,
    all_quantity_sets: &BTreeMap<u32, &QuantitySet>,
    selection_ids: &BTreeMap<u32, String>,
    all_systems: &BTreeMap<u32, IfcSystemMetadata>,
    warnings: &mut BTreeSet<ImportWarning>,
) -> Result<IfcProductMetadata, ImportError> {
    let global_id = normalize_global_id(entity.global_id.clone());
    let selection_id = selection_ids
        .get(&entity.entity_id)
        .cloned()
        .ok_or_else(|| {
            ImportError::InvalidSource(format!(
                "IFC product #{} has no Selection identity",
                entity.entity_id
            ))
        })?;

    let mut property_sets = Vec::new();
    let mut quantity_sets = Vec::new();
    for definition_id in products_by_id.get(&entity.entity_id).into_iter().flatten() {
        if let Some(set) = all_property_sets.get(definition_id) {
            property_sets.push(convert_property_set(set, warnings));
        } else if let Some(set) = all_quantity_sets.get(definition_id) {
            quantity_sets.push(convert_quantity_set(set));
        }
    }
    property_sets.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then(left.express_id.cmp(&right.express_id))
    });
    quantity_sets.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then(left.express_id.cmp(&right.express_id))
    });

    let mut systems = all_systems
        .values()
        .filter(|system| system.members.contains(&selection_id))
        .map(|system| system.reference.clone())
        .collect::<Vec<_>>();
    systems.sort();

    Ok(IfcProductMetadata {
        express_id: entity.entity_id,
        ifc_type: entity.type_name.to_ascii_uppercase(),
        global_id,
        selection_id,
        name: entity
            .name
            .clone()
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| format!("ifc/{}/{}", entity.type_name, entity.entity_id)),
        description: nonempty(entity.description.clone()),
        object_type: nonempty(entity.object_type.clone()),
        predefined_type: nonempty(entity.predefined_type.clone()),
        tag: nonempty(entity.tag.clone()),
        property_sets,
        quantity_sets,
        systems,
    })
}

fn convert_property_set(
    set: &ifc_lite_processing::data_model::PropertySet,
    warnings: &mut BTreeSet<ImportWarning>,
) -> IfcPropertySetMetadata {
    let mut properties = BTreeMap::new();
    for property in &set.properties {
        let Some(value) = property_value(property, warnings) else {
            continue;
        };
        properties.insert(property.property_name.clone(), value);
    }
    IfcPropertySetMetadata {
        express_id: set.pset_id,
        name: set.pset_name.clone(),
        properties,
    }
}

fn convert_quantity_set(set: &QuantitySet) -> IfcQuantitySetMetadata {
    IfcQuantitySetMetadata {
        express_id: set.qset_id,
        name: set.qset_name.clone(),
        quantities: set
            .quantities
            .iter()
            .filter(|quantity| quantity.quantity_value.is_finite())
            .map(|quantity| (quantity.quantity_name.clone(), quantity.quantity_value))
            .collect(),
    }
}

fn property_value(property: &Property, warnings: &mut BTreeSet<ImportWarning>) -> Option<Value> {
    if let Some(values) = &property.values {
        if values.len() > MAX_METADATA_ARRAY_LENGTH {
            warnings.insert(warning(
                "ICE3_STUDIO_IMPORT_IFC_METADATA_TRUNCATED",
                format!(
                    "IFC property {} has {} values; limit is {}",
                    property.property_name,
                    values.len(),
                    MAX_METADATA_ARRAY_LENGTH
                ),
            ));
            return None;
        }
        return Some(Value::Array(
            values.iter().cloned().map(Value::String).collect(),
        ));
    }
    let raw = property.property_value.as_str();
    match property.property_type.as_str() {
        "boolean" | "logical" => match raw {
            "true" => Some(Value::Bool(true)),
            "false" => Some(Value::Bool(false)),
            _ => Some(Value::String(raw.into())),
        },
        "integer" => raw
            .parse::<i64>()
            .ok()
            .map(|value| Value::Number(value.into())),
        "real" => raw
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number),
        "null" => None,
        _ => Some(Value::String(raw.into())),
    }
}

fn build_system_members(
    relationships: &[Relationship],
    entities: &BTreeMap<u32, EntityMetadata>,
) -> BTreeMap<u32, BTreeSet<u32>> {
    let mut systems = BTreeMap::<u32, BTreeSet<u32>>::new();
    for relation in relationships {
        if !is_group_relation(&relation.rel_type) {
            continue;
        }
        let Some(group) = entities.get(&relation.relating_id) else {
            continue;
        };
        if !group.type_name.to_ascii_uppercase().contains("SYSTEM") {
            continue;
        }
        if !entities.contains_key(&relation.related_id) {
            continue;
        }
        systems
            .entry(group.entity_id)
            .or_default()
            .insert(relation.related_id);
    }
    systems
}

fn build_systems(
    system_members: &BTreeMap<u32, BTreeSet<u32>>,
    selection_ids: &BTreeMap<u32, String>,
    entities: &BTreeMap<u32, EntityMetadata>,
) -> BTreeMap<u32, IfcSystemMetadata> {
    system_members
        .iter()
        .filter_map(|(express_id, members)| {
            let entity = entities.get(express_id)?;
            Some((
                *express_id,
                IfcSystemMetadata {
                    reference: IfcSystemReference {
                        express_id: *express_id,
                        global_id: normalize_global_id(entity.global_id.clone()),
                        name: entity
                            .name
                            .clone()
                            .filter(|name| !name.is_empty())
                            .unwrap_or_else(|| format!("IFC System {express_id}")),
                        ifc_type: entity.type_name.to_ascii_uppercase(),
                    },
                    members: members
                        .iter()
                        .filter_map(|member| selection_ids.get(member).cloned())
                        .collect(),
                },
            ))
        })
        .collect()
}

fn build_selection_ids(
    product_ids: &BTreeSet<u32>,
    entities: &BTreeMap<u32, EntityMetadata>,
    asset_id: &str,
    used_global_ids: &mut BTreeSet<String>,
    warnings: &mut BTreeSet<ImportWarning>,
) -> Result<BTreeMap<u32, String>, ImportError> {
    let mut selection_ids = BTreeMap::new();
    for express_id in product_ids {
        let entity = entities.get(express_id).ok_or_else(|| {
            ImportError::InvalidSource(format!("IFC product #{express_id} is missing"))
        })?;
        let global_id = normalize_global_id(entity.global_id.clone());
        let selection_id = match global_id.as_ref() {
            Some(global_id) if used_global_ids.insert(global_id.clone()) => {
                format!("{asset_id}/ifc/{global_id}")
            }
            Some(global_id) => {
                warnings.insert(warning(
                    "ICE3_STUDIO_IMPORT_IFC_DUPLICATE_GLOBAL_ID",
                    format!("duplicate IFC GlobalId {global_id} at #{express_id}"),
                ));
                format!("{asset_id}/ifc/{global_id}/express/{express_id}")
            }
            None => format!("{asset_id}/ifc/express/{express_id}"),
        };
        selection_ids.insert(*express_id, selection_id);
    }
    Ok(selection_ids)
}

fn is_property_relation(value: &str) -> bool {
    value.eq_ignore_ascii_case("IfcRelDefinesByProperties")
}

fn is_group_relation(value: &str) -> bool {
    value.eq_ignore_ascii_case("IfcRelAssignsToGroup")
        || value.eq_ignore_ascii_case("IfcRelAssignsToGroupByFactor")
}

fn sorted_ids(values: &[u32]) -> Vec<u32> {
    let mut values = values.to_vec();
    values.sort_unstable();
    values.dedup();
    values
}

fn nonempty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.is_empty())
}

fn normalize_global_id(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.is_empty() && value != "$")
}

fn warning(code: &str, message: String) -> ImportWarning {
    ImportWarning {
        code: code.into(),
        message,
        path: None,
    }
}
