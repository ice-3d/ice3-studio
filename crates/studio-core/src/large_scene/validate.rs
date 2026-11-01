use std::collections::{BTreeMap, BTreeSet};

use crate::document::{Document, NodeContent};
use crate::ids::ComponentId;
use crate::large_scene::component::{
    ComponentDefinition, ComponentInstance, ComponentParameter, ComponentPort, InstanceLayout,
    InstanceSet, MaterialBinding, ParameterType, ParameterValue, PartContent, PartOverride,
    ScalarBinding, Vector3Binding,
};
use crate::large_scene::connection::{ConnectionIntent, InstancePortRef};
use crate::math::Vec3;
use crate::validation::{Diagnostic, DiagnosticCode};

pub const MAX_COMPONENT_DEPTH: usize = 16;
pub const MAX_COMPONENT_PARTS: usize = 4096;
pub const MAX_INSTANCE_SET_ITEMS: u32 = 100_000;
const MAX_PORTS_PER_COMPONENT: usize = 4096;

#[derive(Debug, thiserror::Error)]
pub enum ParameterError {
    #[error("unknown parameter: {0}")]
    Unknown(String),
    #[error("parameter {0} has the wrong type")]
    WrongType(String),
    #[error("parameter {0} is outside its allowed range")]
    OutOfRange(String),
    #[error("parameter {0} is not finite")]
    NonFinite(String),
    #[error("expression failed: {0}")]
    Expression(String),
}

pub fn resolve_parameters(
    definition: &ComponentDefinition,
    supplied: &BTreeMap<String, ParameterValue>,
) -> Result<BTreeMap<String, ParameterValue>, ParameterError> {
    let mut resolved = definition
        .parameters
        .iter()
        .map(|parameter| (parameter.name.clone(), parameter.default.clone()))
        .collect::<BTreeMap<_, _>>();
    for (name, value) in supplied {
        let declaration = definition
            .parameters
            .iter()
            .find(|parameter| parameter.name == *name)
            .ok_or_else(|| ParameterError::Unknown(name.clone()))?;
        validate_value(declaration, value)?;
        resolved.insert(name.clone(), value.clone());
    }
    Ok(resolved)
}

pub fn validate_large_scene(document: &Document) -> Vec<Diagnostic> {
    let definitions = document
        .components
        .iter()
        .map(|definition| (definition.id.clone(), definition))
        .collect::<BTreeMap<_, _>>();
    let mut diagnostics = Vec::new();
    let mut seen_definitions = BTreeSet::new();

    for (index, definition) in document.components.iter().enumerate() {
        if !seen_definitions.insert(definition.id.clone()) {
            diagnostics.push(component(
                definition.id.as_str(),
                format!("/components/{index}/id"),
                "component definition ID is duplicated",
            ));
        }
        validate_definition(definition, &definitions, &mut diagnostics);
        let mut stack = BTreeSet::new();
        validate_reference_depth(
            &definition.id,
            &definitions,
            0,
            &mut stack,
            &mut diagnostics,
        );
    }

    for (index, node) in document.scene.nodes().iter().enumerate() {
        match &node.content {
            NodeContent::ComponentInstance(instance) => {
                validate_instance(
                    instance,
                    &definitions,
                    index,
                    node.id.as_str(),
                    &mut diagnostics,
                );
            }
            NodeContent::InstanceSet(set) => {
                validate_instance_set(set, &definitions, index, node.id.as_str(), &mut diagnostics);
            }
            NodeContent::Connection(connection) => {
                validate_connection_references(
                    document,
                    connection,
                    index,
                    node.id.as_str(),
                    &definitions,
                    &mut diagnostics,
                );
            }
            _ => {}
        }
    }

    diagnostics
}

pub fn instance_count(layout: &InstanceLayout) -> Option<u32> {
    match layout {
        InstanceLayout::Linear { count, .. } | InstanceLayout::Radial { count, .. } => Some(*count),
        InstanceLayout::Grid { rows, columns, .. } => rows.checked_mul(*columns),
        InstanceLayout::Path { placements } => u32::try_from(placements.len()).ok(),
    }
}

fn validate_definition(
    definition: &ComponentDefinition,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !definition.id.is_valid() {
        diagnostics.push(component(
            definition.id.as_str(),
            "/id",
            "component definition ID is invalid",
        ));
    }
    if definition.parts.len() > MAX_COMPONENT_PARTS {
        diagnostics.push(component(
            definition.id.as_str(),
            "/parts",
            "component has too many parts",
        ));
    }
    if definition.ports.len() > MAX_PORTS_PER_COMPONENT {
        diagnostics.push(component(
            definition.id.as_str(),
            "/ports",
            "component has too many ports",
        ));
    }

    let mut parameters = BTreeMap::new();
    for (index, parameter) in definition.parameters.iter().enumerate() {
        if parameter.name.is_empty()
            || parameters
                .insert(parameter.name.clone(), parameter)
                .is_some()
        {
            diagnostics.push(parameter_diagnostic(
                definition.id.as_str(),
                format!("/parameters/{index}/name"),
                "component parameter names must be unique and non-empty",
            ));
            continue;
        }
        if let Err(error) = validate_value(parameter, &parameter.default) {
            diagnostics.push(parameter_diagnostic(
                definition.id.as_str(),
                format!("/parameters/{index}/default"),
                error.to_string(),
            ));
        }
    }

    let mut parts = BTreeMap::new();
    for (index, part) in definition.parts.iter().enumerate() {
        if part.id.is_empty() || parts.insert(part.id.clone(), part).is_some() {
            diagnostics.push(component(
                definition.id.as_str(),
                format!("/parts/{index}/id"),
                "component part IDs must be unique and non-empty",
            ));
            continue;
        }
    }

    for (index, part) in definition.parts.iter().enumerate() {
        if let Some(parent) = &part.parent_id
            && part.id == *parent
        {
            diagnostics.push(component(
                definition.id.as_str(),
                format!("/parts/{index}/parentId"),
                "component part cannot parent itself",
            ));
        }
        if let Some(parent) = &part.parent_id
            && !parts.contains_key(parent)
        {
            diagnostics.push(component(
                definition.id.as_str(),
                format!("/parts/{index}/parentId"),
                "component parent part does not exist",
            ));
        }
        if let Some(parent) = &part.parent_id {
            let mut seen = BTreeSet::new();
            let mut current = Some(parent);
            while let Some(id) = current {
                if !seen.insert(id.clone()) || id == &part.id {
                    diagnostics.push(component(
                        definition.id.as_str(),
                        format!("/parts/{index}/parentId"),
                        "component part hierarchy contains a cycle",
                    ));
                    break;
                }
                current = parts
                    .get(id)
                    .and_then(|candidate| candidate.parent_id.as_ref());
            }
        }

        validate_part_transform(
            definition.id.as_str(),
            index,
            &part.transform.translation,
            &part.transform.scale,
            &part.transform.rotation,
            &parameters,
            diagnostics,
        );
        validate_part_content(
            definition.id.as_str(),
            index,
            &part.content,
            &parameters,
            definitions,
            diagnostics,
        );
        if let Some(binding) = &part.material_role
            && !binding_is_declared(binding.parameter_name(), &parameters)
        {
            diagnostics.push(parameter_diagnostic(
                definition.id.as_str(),
                format!("/parts/{index}/materialRole"),
                "material binding references an unknown parameter",
            ));
        }
    }

    let mut ports = BTreeSet::new();
    for (index, port) in definition.ports.iter().enumerate() {
        if port.name.is_empty() || !ports.insert(port.name.clone()) {
            diagnostics.push(component(
                definition.id.as_str(),
                format!("/ports/{index}/name"),
                "component port names must be unique and non-empty",
            ));
        }
        if !parts.contains_key(&port.part_id) {
            diagnostics.push(component(
                definition.id.as_str(),
                format!("/ports/{index}/partId"),
                "component port part does not exist",
            ));
        }
        if !vec3_is_finite(port.position)
            || !vec3_is_finite(port.direction)
            || vec3_length_squared(port.direction) <= f32::EPSILON
            || !port.diameter.is_finite()
            || port.diameter <= 0.0
        {
            diagnostics.push(component(
                definition.id.as_str(),
                format!("/ports/{index}"),
                "component port values are invalid",
            ));
        }
    }
}

fn validate_reference_depth(
    definition_id: &ComponentId,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
    depth: usize,
    stack: &mut BTreeSet<ComponentId>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if depth > MAX_COMPONENT_DEPTH {
        diagnostics.push(component(
            definition_id.as_str(),
            "/parts",
            "component reference depth exceeds the limit",
        ));
        return;
    }
    if !stack.insert(definition_id.clone()) {
        diagnostics.push(component(
            definition_id.as_str(),
            "/parts",
            "component reference graph contains a cycle",
        ));
        return;
    }
    if let Some(definition) = definitions.get(definition_id) {
        for part in &definition.parts {
            if let PartContent::ComponentReference {
                definition_id: nested,
                ..
            } = &part.content
                && definitions.contains_key(nested)
            {
                validate_reference_depth(nested, definitions, depth + 1, stack, diagnostics);
            }
        }
    }
    stack.remove(definition_id);
}

fn validate_instance(
    instance: &ComponentInstance,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
    index: usize,
    node_id: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(definition) = definitions.get(&instance.definition_id) else {
        diagnostics.push(instance_diagnostic(
            node_id,
            format!("/scene/nodes/{index}/content/definitionId"),
            "component definition does not exist",
        ));
        return;
    };
    validate_parameter_map(
        definition,
        &instance.parameters,
        node_id,
        format!("/scene/nodes/{index}/content/parameters"),
        diagnostics,
    );
    validate_overrides(definition, &instance.overrides, node_id, index, diagnostics);
}

fn validate_instance_set(
    set: &InstanceSet,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
    index: usize,
    node_id: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(definition) = definitions.get(&set.definition_id) else {
        diagnostics.push(instance_set(
            node_id,
            format!("/scene/nodes/{index}/content/definitionId"),
            "component definition does not exist",
        ));
        return;
    };
    validate_layout(set, index, node_id, diagnostics);
    validate_parameter_map(
        definition,
        &set.shared_parameters,
        node_id,
        format!("/scene/nodes/{index}/content/sharedParameters"),
        diagnostics,
    );
    let count = instance_count(&set.layout).unwrap_or(0);
    for (ordinal, values) in &set.item_overrides {
        if *ordinal >= count {
            diagnostics.push(instance_set(
                node_id,
                format!("/scene/nodes/{index}/content/itemOverrides/{ordinal}"),
                "instance-set override ordinal is outside the layout",
            ));
        }
        validate_parameter_map(
            definition,
            values,
            node_id,
            format!("/scene/nodes/{index}/content/itemOverrides/{ordinal}"),
            diagnostics,
        );
    }
}

fn validate_overrides(
    definition: &ComponentDefinition,
    overrides: &BTreeMap<String, PartOverride>,
    node_id: &str,
    index: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for (part_id, override_value) in overrides {
        if !definition.parts.iter().any(|part| part.id == *part_id) {
            diagnostics.push(instance_diagnostic(
                node_id,
                format!("/scene/nodes/{index}/content/overrides/{part_id}"),
                "instance override part does not exist",
            ));
        }
        validate_parameter_map(
            definition,
            &override_value.parameters,
            node_id,
            format!("/scene/nodes/{index}/content/overrides/{part_id}/parameters"),
            diagnostics,
        );
    }
}

fn validate_parameter_map(
    definition: &ComponentDefinition,
    values: &BTreeMap<String, ParameterValue>,
    object_id: &str,
    pointer: String,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for (name, value) in values {
        let Some(declaration) = definition
            .parameters
            .iter()
            .find(|parameter| parameter.name == *name)
        else {
            diagnostics.push(parameter_diagnostic(
                object_id,
                format!("{pointer}/{name}"),
                "parameter is not declared by the component",
            ));
            continue;
        };
        if let Err(error) = validate_value(declaration, value) {
            diagnostics.push(parameter_diagnostic(
                object_id,
                format!("{pointer}/{name}"),
                error.to_string(),
            ));
        }
    }
}

fn validate_layout(
    set: &InstanceSet,
    index: usize,
    node_id: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let pointer = format!("/scene/nodes/{index}/content/layout");
    let invalid = |diagnostics: &mut Vec<Diagnostic>, message: &str| {
        diagnostics.push(instance_set(node_id, pointer.clone(), message));
    };
    let Some(count) = instance_count(&set.layout) else {
        invalid(diagnostics, "instance layout item count overflows");
        return;
    };
    if count == 0 || count > MAX_INSTANCE_SET_ITEMS {
        invalid(
            diagnostics,
            "instance layout item count must be between one and the configured limit",
        );
    }
    match &set.layout {
        InstanceLayout::Linear {
            origin,
            direction,
            spacing,
            ..
        } => {
            if !vec3_is_finite(*origin)
                || !vec3_is_finite(*direction)
                || vec3_length_squared(*direction) <= f32::EPSILON
                || !spacing.is_finite()
                || *spacing <= 0.0
            {
                invalid(diagnostics, "linear layout values are invalid");
            }
        }
        InstanceLayout::Grid {
            origin,
            x_direction,
            z_direction,
            row_spacing,
            column_spacing,
            ..
        } => {
            if !vec3_is_finite(*origin)
                || !vec3_is_finite(*x_direction)
                || !vec3_is_finite(*z_direction)
                || vec3_length_squared(*x_direction) <= f32::EPSILON
                || vec3_length_squared(*z_direction) <= f32::EPSILON
                || !row_spacing.is_finite()
                || *row_spacing <= 0.0
                || !column_spacing.is_finite()
                || *column_spacing <= 0.0
            {
                invalid(diagnostics, "grid layout values are invalid");
            }
        }
        InstanceLayout::Radial {
            centre,
            radius,
            start_angle_radians,
            ..
        } => {
            if !vec3_is_finite(*centre)
                || !radius.is_finite()
                || *radius <= 0.0
                || !start_angle_radians.is_finite()
            {
                invalid(diagnostics, "radial layout values are invalid");
            }
        }
        InstanceLayout::Path { placements } => {
            if placements.iter().any(|transform| {
                !vec3_is_finite(transform.translation)
                    || !transform.rotation.iter().all(|value| value.is_finite())
                    || transform
                        .rotation
                        .iter()
                        .map(|value| value * value)
                        .sum::<f32>()
                        <= f32::EPSILON
                    || !vec3_is_finite(transform.scale)
                    || transform.scale.x <= 0.0
                    || transform.scale.y <= 0.0
                    || transform.scale.z <= 0.0
            }) {
                invalid(diagnostics, "path layout transform is invalid");
            }
        }
    }
}

fn validate_connection_references(
    document: &Document,
    connection: &ConnectionIntent,
    index: usize,
    node_id: &str,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let pointer = format!("/scene/nodes/{index}/content");
    if !connection.diameter.is_finite() || connection.diameter <= 0.0 {
        diagnostics.push(connection_diagnostic(
            node_id,
            format!("{pointer}/diameter"),
            "connection diameter must be finite and positive",
        ));
    }
    if !document
        .relations
        .iter()
        .any(|relation| relation.id == connection.relation_id)
    {
        diagnostics.push(connection_diagnostic(
            node_id,
            format!("{pointer}/relationId"),
            "connection relation does not exist",
        ));
    }
    if connection.from == connection.to {
        diagnostics.push(connection_diagnostic(
            node_id,
            pointer.clone(),
            "connection endpoints must be distinct",
        ));
    }
    let from_port = resolve_port_definition(document, &connection.from, definitions);
    let to_port = resolve_port_definition(document, &connection.to, definitions);
    for (endpoint, resolved) in [("from", from_port), ("to", to_port)] {
        match resolved {
            Ok(port) if port.medium == connection.medium => {}
            Ok(_) => diagnostics.push(connection_diagnostic(
                node_id,
                format!("{pointer}/{endpoint}"),
                "connection medium does not match its port",
            )),
            Err(message) => diagnostics.push(connection_diagnostic(
                node_id,
                format!("{pointer}/{endpoint}"),
                message,
            )),
        }
    }
}

fn resolve_port_definition<'a>(
    document: &'a Document,
    reference: &InstancePortRef,
    definitions: &BTreeMap<ComponentId, &'a ComponentDefinition>,
) -> Result<&'a ComponentPort, &'static str> {
    let (node_id, ordinal, port_name) = match reference {
        InstancePortRef::Instance { node_id, port } => (node_id, None, port),
        InstancePortRef::SetItem {
            set_node_id,
            ordinal,
            port,
        } => (set_node_id, Some(*ordinal), port),
    };
    let node = document
        .scene
        .nodes()
        .iter()
        .find(|candidate| candidate.id == *node_id && candidate.visible)
        .ok_or("connection endpoint does not exist")?;
    let definition_id = match (&node.content, ordinal) {
        (NodeContent::ComponentInstance(instance), None) => &instance.definition_id,
        (NodeContent::InstanceSet(set), Some(ordinal)) => {
            if ordinal >= instance_count(&set.layout).unwrap_or(0) {
                return Err("connection instance-set ordinal does not exist");
            }
            &set.definition_id
        }
        _ => return Err("connection endpoint has the wrong node type"),
    };
    let definition = definitions
        .get(definition_id)
        .ok_or("connection component definition does not exist")?;
    definition
        .ports
        .iter()
        .find(|port| port.name == *port_name)
        .ok_or("connection port does not exist")
}

fn validate_part_content(
    definition_id: &str,
    index: usize,
    content: &PartContent,
    parameters: &BTreeMap<String, &ComponentParameter>,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let pointer = format!("/parts/{index}/content");
    let invalid_binding = |diagnostics: &mut Vec<Diagnostic>, name: &str| {
        diagnostics.push(parameter_diagnostic(
            definition_id,
            format!("{pointer}/{name}"),
            "geometry binding references an unknown parameter",
        ));
    };
    match content {
        PartContent::Box {
            width,
            height,
            depth,
        } => {
            for (name, binding) in [("width", width), ("height", height), ("depth", depth)] {
                if !scalar_binding_is_declared(binding, parameters) {
                    invalid_binding(diagnostics, name);
                }
            }
        }
        PartContent::Plane { width, depth } => {
            for (name, binding) in [("width", width), ("depth", depth)] {
                if !scalar_binding_is_declared(binding, parameters) {
                    invalid_binding(diagnostics, name);
                }
            }
        }
        PartContent::Cylinder {
            radius,
            height,
            segments,
        } => {
            for (name, binding) in [("radius", radius), ("height", height)] {
                if !scalar_binding_is_declared(binding, parameters) {
                    invalid_binding(diagnostics, name);
                }
            }
            if !(3..=256).contains(segments) {
                diagnostics.push(component(
                    definition_id,
                    format!("{pointer}/segments"),
                    "cylinder segment count is outside the supported range",
                ));
            }
        }
        PartContent::Sphere {
            radius,
            segments,
            rings,
        } => {
            if !scalar_binding_is_declared(radius, parameters) {
                invalid_binding(diagnostics, "radius");
            }
            if !(3..=256).contains(segments) || !(2..=128).contains(rings) {
                diagnostics.push(component(
                    definition_id,
                    pointer,
                    "sphere segment or ring count is outside the supported range",
                ));
            }
        }
        PartContent::Sweep {
            points,
            radius,
            radial_segments,
        } => {
            if !scalar_binding_is_declared(radius, parameters) {
                invalid_binding(diagnostics, "radius");
            }
            if points.len() < 2
                || !(3..=128).contains(radial_segments)
                || points.iter().flatten().any(|value| !value.is_finite())
            {
                diagnostics.push(component(
                    definition_id,
                    pointer,
                    "sweep content is invalid",
                ));
            }
        }
        PartContent::ComponentReference {
            definition_id: nested,
            parameters: supplied,
        } => {
            if let Some(target) = definitions.get(nested) {
                validate_parameter_map(
                    target,
                    supplied,
                    definition_id,
                    format!("{pointer}/parameters"),
                    diagnostics,
                );
            } else {
                diagnostics.push(component(
                    definition_id,
                    format!("{pointer}/definitionId"),
                    "nested component definition does not exist",
                ));
            }
        }
        PartContent::Group | PartContent::CustomMesh { .. } => {}
    }
}

fn validate_part_transform(
    definition_id: &str,
    index: usize,
    translation: &Vector3Binding,
    scale: &Vector3Binding,
    rotation: &[f32; 4],
    parameters: &BTreeMap<String, &ComponentParameter>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !binding_is_declared(translation.parameter_name(), parameters)
        || !binding_is_declared(scale.parameter_name(), parameters)
        || !rotation.iter().all(|value| value.is_finite())
        || rotation.iter().map(|value| value * value).sum::<f32>() <= f32::EPSILON
    {
        diagnostics.push(parameter_diagnostic(
            definition_id,
            format!("/parts/{index}/transform"),
            "part transform binding is invalid",
        ));
    }
}

fn validate_value(
    parameter: &ComponentParameter,
    value: &ParameterValue,
) -> Result<(), ParameterError> {
    match (&parameter.value_type, value) {
        (ParameterType::Number { minimum, maximum }, ParameterValue::Number(value)) => {
            if !value.is_finite() {
                Err(ParameterError::NonFinite(parameter.name.clone()))
            } else if minimum.is_some_and(|minimum| *value < minimum)
                || maximum.is_some_and(|maximum| *value > maximum)
            {
                Err(ParameterError::OutOfRange(parameter.name.clone()))
            } else {
                Ok(())
            }
        }
        (ParameterType::Vector3, ParameterValue::Vector3(value)) => {
            if vec3_is_finite(*value) {
                Ok(())
            } else {
                Err(ParameterError::NonFinite(parameter.name.clone()))
            }
        }
        (ParameterType::Color, ParameterValue::Color(value)) => {
            if value.iter().all(|component| component.is_finite())
                && value
                    .iter()
                    .all(|component| (0.0..=1.0).contains(component))
            {
                Ok(())
            } else {
                Err(ParameterError::NonFinite(parameter.name.clone()))
            }
        }
        (ParameterType::Enumeration { values }, ParameterValue::Enumeration(value))
            if values.contains(value) =>
        {
            Ok(())
        }
        _ => Err(ParameterError::WrongType(parameter.name.clone())),
    }
}

fn scalar_binding_is_declared(
    binding: &ScalarBinding,
    parameters: &BTreeMap<String, &ComponentParameter>,
) -> bool {
    binding_is_declared(binding.parameter_name(), parameters)
}

fn binding_is_declared(
    name: Option<&str>,
    parameters: &BTreeMap<String, &ComponentParameter>,
) -> bool {
    name.is_none_or(|name| parameters.contains_key(name))
}

fn vec3_is_finite(value: Vec3) -> bool {
    value.x.is_finite() && value.y.is_finite() && value.z.is_finite()
}

fn vec3_length_squared(value: Vec3) -> f32 {
    value.x * value.x + value.y * value.y + value.z * value.z
}

fn component(
    object_id: &str,
    pointer: impl Into<String>,
    message: impl Into<String>,
) -> Diagnostic {
    diagnostic(
        DiagnosticCode::ComponentInvalid,
        object_id,
        pointer,
        message,
    )
}

fn parameter_diagnostic(
    object_id: &str,
    pointer: impl Into<String>,
    message: impl Into<String>,
) -> Diagnostic {
    diagnostic(
        DiagnosticCode::ParameterInvalid,
        object_id,
        pointer,
        message,
    )
}

fn instance_diagnostic(
    object_id: &str,
    pointer: impl Into<String>,
    message: impl Into<String>,
) -> Diagnostic {
    diagnostic(DiagnosticCode::InstanceInvalid, object_id, pointer, message)
}

fn instance_set(
    object_id: &str,
    pointer: impl Into<String>,
    message: impl Into<String>,
) -> Diagnostic {
    diagnostic(
        DiagnosticCode::InstanceSetInvalid,
        object_id,
        pointer,
        message,
    )
}

fn connection_diagnostic(
    object_id: &str,
    pointer: impl Into<String>,
    message: impl Into<String>,
) -> Diagnostic {
    diagnostic(
        DiagnosticCode::ConnectionInvalid,
        object_id,
        pointer,
        message,
    )
}

fn diagnostic(
    code: DiagnosticCode,
    object_id: &str,
    pointer: impl Into<String>,
    message: impl Into<String>,
) -> Diagnostic {
    Diagnostic {
        code,
        operation_index: None,
        object_id: Some(object_id.to_owned()),
        pointer: pointer.into(),
        message: message.into(),
    }
}

trait BindingName {
    fn parameter_name(&self) -> Option<&str>;
}

impl BindingName for ScalarBinding {
    fn parameter_name(&self) -> Option<&str> {
        match self {
            ScalarBinding::Literal(_) => None,
            ScalarBinding::Parameter { param: name } => Some(name),
            // 表达式可以引用多个参数，无法归到单一名字；保守返回 None，
            // 参数变化时按“整体重算”处理。
            ScalarBinding::Expression { .. } => None,
        }
    }
}

impl BindingName for Vector3Binding {
    fn parameter_name(&self) -> Option<&str> {
        match self {
            Vector3Binding::Literal(_) => None,
            Vector3Binding::Parameter { param: name } => Some(name),
            Vector3Binding::Components { .. } => None,
        }
    }
}

impl BindingName for MaterialBinding {
    fn parameter_name(&self) -> Option<&str> {
        match self {
            MaterialBinding::Literal(_) => None,
            MaterialBinding::Parameter { param: name } => Some(name),
        }
    }
}
