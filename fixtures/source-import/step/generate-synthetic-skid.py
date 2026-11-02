#!/usr/bin/env python3

import sys
from pathlib import Path
import re

import FreeCAD as App
import Import
import Part


COLORS = (
    (0.16, 0.48, 0.72),
    (0.92, 0.56, 0.12),
)


def main(output_path: str, empty_geometry: bool = False) -> None:
    document = App.newDocument("SyntheticSkid")
    assembly_name = "EmptyAssembly" if empty_geometry else "SyntheticSkid"
    assembly = document.addObject("App::Part", assembly_name)
    assembly.Label = assembly_name

    if empty_geometry:
        wire = document.addObject("Part::Feature", "ReferenceCurve")
        wire.Label = "ReferenceCurve"
        wire.Shape = Part.makePolygon([
            App.Vector(0.0, 0.0, 0.0),
            App.Vector(1000.0, 0.0, 0.0),
        ])
        assembly.addObject(wire)
        document.recompute()
        Import.export([assembly], output_path)
        App.closeDocument(document.Name)
        normalize_step_file(Path(output_path))
        return

    tank_shape = Part.makeBox(1200.0, 700.0, 900.0)
    tank = document.addObject("Part::Feature", "PrimaryTank")
    tank.Label = "PrimaryTank"
    tank.Shape = tank_shape
    assembly.addObject(tank)

    pump_shape = Part.makeCylinder(260.0, 480.0)
    pump_shape.translate(App.Vector(1750.0, 0.0, 0.0))
    pump = document.addObject("Part::Feature", "TransferPump")
    pump.Label = "TransferPump"
    pump.Shape = pump_shape
    assembly.addObject(pump)

    document.recompute()
    set_shape_color(tank, (0.16, 0.48, 0.72))
    set_shape_color(pump, (0.92, 0.56, 0.12))

    Import.export([assembly], output_path)
    App.closeDocument(document.Name)
    add_ap242_colors(Path(output_path))


def set_shape_color(feature, color) -> None:
    if not hasattr(feature, "ShapeColor"):
        feature.addProperty("App::PropertyColor", "ShapeColor", "Base")
    feature.ShapeColor = color
    view_object = getattr(feature, "ViewObject", None)
    if view_object is not None:
        view_object.ShapeColor = color


def normalize_step_file(output_path: Path) -> None:
    source = output_path.read_text(encoding="utf-8")
    source = "\n".join(line.rstrip() for line in source.splitlines()) + "\n"
    source = re.sub(
        r"FILE_SCHEMA\(\(.*?\)\);",
        "FILE_SCHEMA(('AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF'));",
        source,
        count=1,
        flags=re.DOTALL,
    )
    source = source.replace(
        "'automotive_design',2000",
        "'managed_model_based_3d_engineering',2014",
        1,
    )
    output_path.write_text(source, encoding="utf-8")


def add_ap242_colors(output_path: Path) -> None:
    normalize_step_file(output_path)
    source = output_path.read_text(encoding="utf-8")
    solid_ids = [
        int(value)
        for value in re.findall(
            r"#(\d+)\s*=\s*MANIFOLD_SOLID_BREP",
            source,
        )
    ]
    if len(solid_ids) != 2:
        raise RuntimeError(f"expected two solids, found {len(solid_ids)}")
    next_id = max(int(value) for value in re.findall(r"#(\d+)\s*=", source)) + 1
    entities = []
    for solid_id, color in zip(solid_ids, COLORS):
        entity_ids = list(range(next_id, next_id + 8))
        next_id += 8
        entities.extend([
            f"#{entity_ids[0]} = COLOUR_RGB('',{color[0]:.8g},{color[1]:.8g},{color[2]:.8g});",
            f"#{entity_ids[1]} = FILL_AREA_STYLE_COLOUR('',#{entity_ids[0]});",
            f"#{entity_ids[2]} = FILL_AREA_STYLE('',(#{entity_ids[1]}));",
            f"#{entity_ids[3]} = SURFACE_STYLE_FILL_AREA(#{entity_ids[2]});",
            f"#{entity_ids[4]} = SURFACE_SIDE_STYLE('',(#{entity_ids[3]}));",
            f"#{entity_ids[5]} = SURFACE_STYLE_USAGE(.BOTH.,#{entity_ids[4]});",
            f"#{entity_ids[6]} = PRESENTATION_STYLE_ASSIGNMENT((#{entity_ids[5]}));",
            f"#{entity_ids[7]} = STYLED_ITEM('',(#{entity_ids[6]}),#{solid_id});",
        ])
    body, ending = source.rsplit("ENDSEC;", 1)
    entity_text = "\n".join(entities)
    output_path.write_text(
        f"{body}{entity_text}\nENDSEC;{ending}",
        encoding="utf-8",
    )


if __name__ == "__main__":
    if len(sys.argv) not in (2, 3):
        raise SystemExit(
            "usage: generate-synthetic-skid.py OUTPUT.step [--empty]"
        )
    main(
        sys.argv[1],
        empty_geometry=len(sys.argv) == 3 and sys.argv[2] == "--empty",
    )
