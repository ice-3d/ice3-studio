#!/usr/bin/env python3

import argparse
import os
import sys

from pxr import Gf, Sdf, Usd, UsdGeom, UsdShade


CUBE_POINTS = [
    (-1, -1, -1),
    (-1, -1, 1),
    (-1, 1, -1),
    (-1, 1, 1),
    (1, -1, -1),
    (1, -1, 1),
    (1, 1, -1),
    (1, 1, 1),
]
CUBE_COUNTS = [3] * 12
CUBE_INDICES = [
    1, 2, 0, 3, 6, 2, 7, 4, 6, 5, 0, 4, 6, 0, 2, 3, 5, 7,
    1, 3, 2, 3, 7, 6, 7, 5, 4, 5, 1, 0, 6, 4, 0, 3, 1, 5,
]


def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output-dir", required=True)
    return parser.parse_args(sys.argv[sys.argv.index("--") + 1:])


def add_material(stage, path, color):
    material = UsdShade.Material.Define(stage, path)
    shader = UsdShade.Shader.Define(stage, f"{path}/PreviewSurface")
    shader.CreateIdAttr("UsdPreviewSurface")
    shader.CreateInput("diffuseColor", Sdf.ValueTypeNames.Color3f).Set(color)
    shader.CreateInput("metallic", Sdf.ValueTypeNames.Float).Set(0.0)
    shader.CreateInput("roughness", Sdf.ValueTypeNames.Float).Set(1.0)
    material.CreateSurfaceOutput().ConnectToSource(
        shader.ConnectableAPI(),
        "surface",
    )
    return material


def add_box_mesh(stage, path, color, material, tag, equipment_class):
    mesh = UsdGeom.Mesh.Define(stage, path)
    mesh.CreatePointsAttr([Gf.Vec3f(*point) for point in CUBE_POINTS])
    mesh.CreateFaceVertexCountsAttr(CUBE_COUNTS)
    mesh.CreateFaceVertexIndicesAttr(CUBE_INDICES)
    mesh.CreateSubdivisionSchemeAttr(UsdGeom.Tokens.none)
    mesh.CreateExtentAttr([
        Gf.Vec3f(-1, -1, -1),
        Gf.Vec3f(1, 1, 1),
    ])
    colors = UsdGeom.PrimvarsAPI(mesh).CreatePrimvar(
        "displayColor",
        Sdf.ValueTypeNames.Color3fArray,
        UsdGeom.Tokens.constant,
    )
    colors.Set([color])
    UsdShade.MaterialBindingAPI.Apply(
        mesh.GetPrim(),
    ).Bind(material)
    mesh.GetPrim().CreateAttribute(
        "userProperties:EquipmentTag",
        Sdf.ValueTypeNames.String,
    ).Set(tag)
    mesh.GetPrim().CreateAttribute(
        "userProperties:EquipmentClass",
        Sdf.ValueTypeNames.String,
    ).Set(equipment_class)
    return mesh


def build_stage(path, origin=(0.0, 0.0, 0.0)):
    stage = Usd.Stage.CreateNew(path)
    UsdGeom.SetStageUpAxis(stage, UsdGeom.Tokens.y)
    UsdGeom.SetStageMetersPerUnit(stage, 1.0)

    plant = UsdGeom.Xform.Define(stage, "/Plant")
    stage.SetDefaultPrim(plant.GetPrim())
    line = UsdGeom.Xform.Define(stage, "/Plant/TreatmentLine")

    pump_material = add_material(
        stage,
        "/Plant/_materials/PumpBlue",
        Gf.Vec3f(0.12, 0.38, 0.68),
    )
    tank_material = add_material(
        stage,
        "/Plant/_materials/TankTaupe",
        Gf.Vec3f(0.62, 0.55, 0.40),
    )

    pump = add_box_mesh(
        stage,
        "/Plant/TreatmentLine/PrimaryPump",
        Gf.Vec3f(0.12, 0.38, 0.68),
        pump_material,
        "P-101",
        "pump",
    )
    pump_xform = UsdGeom.Xformable(pump.GetPrim())
    pump_xform.AddTranslateOp().Set(Gf.Vec3d(
        origin[0] + 2.0,
        origin[1] + 1.5,
        origin[2] + 0.8,
    ))
    pump_xform.AddScaleOp().Set(Gf.Vec3f(1.2, 0.65, 0.8))

    instance = UsdGeom.Xform.Define(
        stage,
        "/Plant/TreatmentLine/PumpInstanceA",
    )
    instance_prim = instance.GetPrim()
    instance_prim.GetReferences().AddInternalReference(
        "/Plant/TreatmentLine/PrimaryPump",
    )
    instance_prim.SetInstanceable(True)
    instance_prim.GetAttribute("xformOp:translate").Set(
        Gf.Vec3d(origin[0] - 2.0, origin[1] + 1.5, origin[2] + 0.8),
    )

    tank = add_box_mesh(
        stage,
        "/Plant/TreatmentLine/BufferTank",
        Gf.Vec3f(0.62, 0.55, 0.40),
        tank_material,
        "T-101",
        "tank",
    )
    tank_xform = UsdGeom.Xformable(tank.GetPrim())
    tank_xform.AddTranslateOp().Set(Gf.Vec3d(
        origin[0],
        origin[1] - 2.2,
        origin[2] + 1.8,
    ))
    tank_xform.AddScaleOp().Set(Gf.Vec3f(1.4, 1.0, 1.8))

    line.GetPrim().CreateAttribute(
        "userProperties:ProcessArea",
        Sdf.ValueTypeNames.String,
    ).Set("water-line")
    stage.GetRootLayer().Save()


def main():
    args = parse_args()
    os.makedirs(args.output_dir, exist_ok=True)
    ascii_path = os.path.join(args.output_dir, "synthetic-plant.usda")
    build_stage(ascii_path)
    build_stage(os.path.join(args.output_dir, "synthetic-plant.usdc"))
    large_origin_path = os.path.join(args.output_dir, "large-origin.usda")
    build_stage(
        large_origin_path,
        (1_000_010.0, 0.0, 500_000.0),
    )
    for path in (ascii_path, large_origin_path):
        with open(path, encoding="utf-8") as source:
            content = source.read()
        with open(path, "w", encoding="utf-8", newline="\n") as destination:
            destination.write(content.rstrip() + "\n")


if __name__ == "__main__":
    main()
