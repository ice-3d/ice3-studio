#!/usr/bin/env python3

import sys
import re
from pathlib import Path

import ifcopenshell
import ifcopenshell.api
import ifcopenshell.guid


def create_model(
    length_unit: bool = True,
    schema: str = "IFC4X3",
    origin_offset: float = 0.0,
    with_products: bool = True,
    with_orphan: bool = False,
    conflicting_units: bool = False,
) -> ifcopenshell.file:
    model = ifcopenshell.api.run("project.create_file", version=schema)
    project = ifcopenshell.api.run(
        "root.create_entity",
        model,
        ifc_class="IfcProject",
        name="Synthetic IFC Plant",
    )
    site = ifcopenshell.api.run(
        "root.create_entity",
        model,
        ifc_class="IfcSite",
        name="Synthetic Site",
    )
    building = ifcopenshell.api.run(
        "root.create_entity",
        model,
        ifc_class="IfcBuilding",
        name="Treatment Building",
    )
    storey = ifcopenshell.api.run(
        "root.create_entity",
        model,
        ifc_class="IfcBuildingStorey",
        name="Process Floor",
    )
    ifcopenshell.api.run(
        "aggregate.assign_object",
        model,
        products=[site],
        relating_object=project,
    )
    ifcopenshell.api.run(
        "aggregate.assign_object",
        model,
        products=[building],
        relating_object=site,
    )
    ifcopenshell.api.run(
        "aggregate.assign_object",
        model,
        products=[storey],
        relating_object=building,
    )

    area_unit = ifcopenshell.api.run(
        "unit.add_si_unit",
        model,
        unit_type="AREAUNIT",
    )
    volume_unit = ifcopenshell.api.run(
        "unit.add_si_unit",
        model,
        unit_type="VOLUMEUNIT",
    )
    units = [area_unit, volume_unit]
    if length_unit:
        units.insert(
            0,
            ifcopenshell.api.run(
                "unit.add_si_unit",
                model,
                unit_type="LENGTHUNIT",
                prefix="MILLI",
            ),
        )
        if conflicting_units:
            units.append(ifcopenshell.api.run(
                "unit.add_si_unit",
                model,
                unit_type="LENGTHUNIT",
                prefix=None,
            ))
    ifcopenshell.api.run("unit.assign_unit", model, units=units)
    if not with_products:
        return model

    model_context = ifcopenshell.api.run(
        "context.add_context",
        model,
        context_type="Model",
    )
    body_context = ifcopenshell.api.run(
        "context.add_context",
        model,
        context_type="Model",
        context_identifier="Body",
        target_view="MODEL_VIEW",
        parent=model_context,
    )

    pump = ifcopenshell.api.run(
        "root.create_entity",
        model,
        ifc_class="IfcPump",
        predefined_type="CENTRIFUGAL",
        name="Primary Pump",
    )
    tank = ifcopenshell.api.run(
        "root.create_entity",
        model,
        ifc_class="IfcTank",
        predefined_type="BASIN",
        name="Buffer Tank",
    )
    ifcopenshell.api.run(
        "spatial.assign_container",
        model,
        products=[pump, tank],
        relating_structure=storey,
    )

    pump.Tag = "P-101"
    tank.Tag = "T-201"

    pump_representation = ifcopenshell.api.run(
        "geometry.add_wall_representation",
        model,
        context=body_context,
        length=1.4,
        height=0.8,
        thickness=0.5,
    )
    tank_representation = ifcopenshell.api.run(
        "geometry.add_wall_representation",
        model,
        context=body_context,
        length=2.0,
        height=1.2,
        thickness=1.0,
    )
    ifcopenshell.api.run(
        "geometry.assign_representation",
        model,
        product=pump,
        representation=pump_representation,
    )
    ifcopenshell.api.run(
        "geometry.assign_representation",
        model,
        product=tank,
        representation=tank_representation,
    )
    ifcopenshell.api.run(
        "geometry.edit_object_placement",
        model,
        product=pump,
        matrix=[
            [1.0, 0.0, 0.0, 2.0 + origin_offset],
            [0.0, 1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    )
    ifcopenshell.api.run(
        "geometry.edit_object_placement",
        model,
        product=tank,
        matrix=[
            [1.0, 0.0, 0.0, 5.0 + origin_offset],
            [0.0, 1.0, 0.0, 2.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    )

    for product, name, color in (
        (pump, "Pump Blue", (0.12, 0.42, 0.78)),
        (tank, "Tank Orange", (0.92, 0.46, 0.08)),
    ):
        style = ifcopenshell.api.run("style.add_style", model, name=name)
        ifcopenshell.api.run(
            "style.add_surface_style",
            model,
            style=style,
            ifc_class="IfcSurfaceStyleShading",
            attributes={
                "SurfaceColour": {
                    "Name": None,
                    "Red": color[0],
                    "Green": color[1],
                    "Blue": color[2],
                },
                "Transparency": 0.0,
            },
        )
        representation = (
            pump.Representation.Representations[0]
            if product is pump
            else tank.Representation.Representations[0]
        )
        ifcopenshell.api.run(
            "style.assign_representation_styles",
            model,
            shape_representation=representation,
            styles=[style],
        )

    pump_pset = ifcopenshell.api.run(
        "pset.add_pset",
        model,
        product=pump,
        name="Pset_PumpTypeCommon",
    )
    ifcopenshell.api.run(
        "pset.edit_pset",
        model,
        pset=pump_pset,
        properties={
            "FlowRate": 12.5,
            "PumpType": "CENTRIFUGAL",
            "IsVariableSpeed": True,
        },
    )
    tank_pset = ifcopenshell.api.run(
        "pset.add_pset",
        model,
        product=tank,
        name="Pset_TankTypeCommon",
    )
    ifcopenshell.api.run(
        "pset.edit_pset",
        model,
        pset=tank_pset,
        properties={
            "NominalCapacity": 20.0,
            "StorageType": "LIQUID",
        },
    )
    quantity = ifcopenshell.api.run(
        "pset.add_qto",
        model,
        product=pump,
        name="Qto_PumpBaseQuantities",
    )
    ifcopenshell.api.run(
        "pset.edit_qto",
        model,
        qto=quantity,
        properties={
            "GrossWeight": 480.0,
            "Length": 1400.0,
        },
    )

    system = ifcopenshell.api.run(
        "system.add_system",
        model,
        ifc_class="IfcDistributionSystem",
    )
    system.Name = "Sludge Transfer"
    system.PredefinedType = "WASTEWATER"
    ifcopenshell.api.run(
        "system.assign_system",
        model,
        products=[pump, tank],
        system=system,
    )
    if with_orphan:
        orphan = ifcopenshell.api.run(
            "root.create_entity",
            model,
            ifc_class="IfcPump",
            predefined_type="CENTRIFUGAL",
            name="Orphan Pump",
        )
        orphan.Tag = "P-ORPHAN"
        orphan_representation = ifcopenshell.api.run(
            "geometry.add_wall_representation",
            model,
            context=body_context,
            length=1.0,
            height=0.6,
            thickness=0.4,
        )
        ifcopenshell.api.run(
            "geometry.assign_representation",
            model,
            product=orphan,
            representation=orphan_representation,
        )
        ifcopenshell.api.run(
            "geometry.edit_object_placement",
            model,
            product=orphan,
            matrix=[
                [1.0, 0.0, 0.0, 9.0 + origin_offset],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        )
    return model


def main(output_directory: str) -> None:
    root = Path(output_directory)
    root.mkdir(parents=True, exist_ok=True)
    fixtures = {
        "synthetic-plant.ifc": create_model(),
        "unsupported-unit.ifc": create_model(length_unit=False),
        "conflicting-units.ifc": create_model(conflicting_units=True),
        "ifc4-legacy.ifc": create_model(schema="IFC4"),
        "empty-model.ifc": create_model(with_products=False),
        "large-origin.ifc": create_model(origin_offset=1_000_000.0),
        "orphan-product.ifc": create_model(with_orphan=True),
    }
    for name, model in fixtures.items():
        model.write((root / name).as_posix())
    write_truncated(
        root / "synthetic-plant.ifc",
        root / "truncated.ifc",
    )
    write_global_id_variant(
        root / "synthetic-plant.ifc",
        root / "duplicate-global-id.ifc",
    )
    write_missing_global_id_variant(
        root / "synthetic-plant.ifc",
        root / "missing-global-id.ifc",
    )
    write_semantic_only_variant(
        root / "synthetic-plant.ifc",
        root / "semantic-only-product.ifc",
    )


def write_truncated(source_path: Path, output_path: Path) -> None:
    synthetic = source_path.read_text(encoding="utf-8")
    header_start = synthetic.find("HEADER;")
    data_start = synthetic.find("DATA;")
    if header_start < 0 or data_start < 0:
        raise RuntimeError("unable to create truncated fixture")
    header_start += len("HEADER;")
    truncation = header_start + (data_start - header_start) // 2
    output_path.write_text(
        synthetic[:truncation],
        encoding="utf-8",
    )


def write_global_id_variant(
    source_path: Path,
    output_path: Path,
) -> None:
    model = ifcopenshell.open(source_path.as_posix())
    pump = model.by_type("IfcPump")[0]
    tank = model.by_type("IfcTank")[0]
    tank.GlobalId = pump.GlobalId
    model.write(output_path.as_posix())


def write_missing_global_id_variant(
    source_path: Path,
    output_path: Path,
) -> None:
    source = source_path.read_text(encoding="utf-8")
    source = re.sub(
        r"(=IFCTANK\(')[^']+(')",
        r"\1$\2",
        source,
        count=1,
    )
    output_path.write_text(source, encoding="utf-8")


def write_semantic_only_variant(
    source_path: Path,
    output_path: Path,
) -> None:
    model = ifcopenshell.open(source_path.as_posix())
    pump = model.by_type("IfcPump")[0]
    pump.Representation = None
    model.write(output_path.as_posix())


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: generate-synthetic-plant.py OUTPUT_DIRECTORY")
    main(sys.argv[1])
