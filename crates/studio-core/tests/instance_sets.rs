use studio_core::large_scene::component::InstanceLayout;
use studio_core::large_scene::evaluate::{LayoutError, expand_instance_layout};
use studio_core::large_scene::validate::MAX_INSTANCE_SET_ITEMS;
use studio_core::math::{Transform, Vec3};

#[test]
fn expands_linear_and_grid_layouts_in_stable_order() {
    let linear = expand_instance_layout(&InstanceLayout::Linear {
        origin: Vec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        direction: Vec3 {
            x: 1.0,
            y: 0.0,
            z: 0.0,
        },
        spacing: 2.0,
        count: 3,
    })
    .unwrap();
    assert_eq!(linear.len(), 3);
    assert_eq!(linear[2].translation.x, 4.0);

    let grid = expand_instance_layout(&InstanceLayout::Grid {
        origin: Vec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        x_direction: Vec3 {
            x: 1.0,
            y: 0.0,
            z: 0.0,
        },
        z_direction: Vec3 {
            x: 0.0,
            y: 0.0,
            z: 1.0,
        },
        rows: 2,
        columns: 3,
        row_spacing: 4.0,
        column_spacing: 2.0,
    })
    .unwrap();
    assert_eq!(grid.len(), 6);
    assert_eq!(grid[3].translation.x, 0.0);
    assert_eq!(grid[3].translation.z, 4.0);
}

#[test]
fn rejects_zero_counts_and_non_finite_spacing() {
    assert!(
        expand_instance_layout(&InstanceLayout::Linear {
            origin: Vec3::default(),
            direction: Vec3 {
                x: 1.0,
                y: 0.0,
                z: 0.0,
            },
            spacing: f32::NAN,
            count: 2,
        })
        .is_err()
    );
    assert!(
        expand_instance_layout(&InstanceLayout::Radial {
            centre: Vec3::default(),
            radius: 2.0,
            start_angle_radians: 0.0,
            count: 0,
        })
        .is_err()
    );
}

#[test]
fn rejects_path_layouts_above_the_item_limit() {
    let result = expand_instance_layout(&InstanceLayout::Path {
        placements: vec![Transform::IDENTITY; MAX_INSTANCE_SET_ITEMS as usize + 1],
    });
    assert!(matches!(result, Err(LayoutError::TooManyItems)));
}
