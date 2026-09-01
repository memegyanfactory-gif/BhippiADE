//! Deterministic layout helpers (ENG-126).
//!
//! "Scatter forty crates around the warehouse" is a request a model should answer with one
//! call, not forty hand-picked coordinates it invented — which it does badly, and which
//! makes the result impossible to reproduce or review.
//!
//! Every helper here takes a **seed** and is pure: the same seed and arguments always give
//! the same positions, on any machine. That is what makes a generated level a thing you can
//! regenerate, diff and undo rather than a one-off accident.

use crate::error::{EngineError, Result};

/// A tiny deterministic PRNG (SplitMix64).
///
/// `rand` is not a dependency of this crate and does not need to be: nothing here is
/// cryptographic, and pinning the exact algorithm is the point — swapping generators would
/// silently change every previously generated layout.
#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
}

impl Rng {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        // A zero seed would make SplitMix64 emit a fixed sequence starting at zero; the
        // golden-ratio constant keeps a caller's "seed: 0" as usable as any other.
        Self {
            state: seed ^ 0x9E37_79B9_7F4A_7C15,
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A float in `[0, 1)`.
    pub fn unit(&mut self) -> f32 {
        // 24 bits is exactly f32's mantissa, so every value is representable and the
        // distribution has no gaps.
        (self.next_u64() >> 40) as f32 / (1u32 << 24) as f32
    }

    /// A float in `[low, high)`. Returns `low` when the range is empty.
    pub fn range(&mut self, low: f32, high: f32) -> f32 {
        if high <= low {
            return low;
        }
        (high - low).mul_add(self.unit(), low)
    }
}

/// An axis-aligned area to generate within, on the XZ plane at height `y`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

/// One cuboid placement produced by the architectural helpers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub position: [f32; 3],
    /// Rotation around world Y, in radians.
    pub yaw: f32,
    pub scale: [f32; 3],
}

/// A door/window cut into one of a room's four walls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RoomWall {
    North,
    South,
    East,
    West,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WallOpening {
    pub wall: RoomWall,
    /// Distance along the wall from its centre.
    pub offset: f32,
    pub width: f32,
    /// Clear height measured from the room floor.
    pub height: f32,
}

/// The generated walls plus the exact centre-line endpoints used to join two rooms.
#[derive(Clone, Debug, PartialEq)]
pub struct CorridorLayout {
    pub placements: Vec<Placement>,
    pub from: [f32; 3],
    pub to: [f32; 3],
}

impl Bounds {
    pub fn new(min: [f32; 3], max: [f32; 3]) -> Result<Self> {
        for axis in 0..3 {
            if !(min[axis].is_finite() && max[axis].is_finite()) {
                return Err(EngineError::Action(
                    "bounds must be finite numbers".to_owned(),
                    Some("Give a real min and max.".to_owned()),
                ));
            }
            if max[axis] < min[axis] {
                return Err(EngineError::Action(
                    format!("bounds are inverted on axis {axis}"),
                    Some("min must not exceed max on any axis.".to_owned()),
                ));
            }
        }
        Ok(Self { min, max })
    }

    #[must_use]
    pub fn size(&self) -> [f32; 3] {
        [
            self.max[0] - self.min[0],
            self.max[1] - self.min[1],
            self.max[2] - self.min[2],
        ]
    }

    #[must_use]
    pub fn center(&self) -> [f32; 3] {
        [
            f32::midpoint(self.min[0], self.max[0]),
            f32::midpoint(self.min[1], self.max[1]),
            f32::midpoint(self.min[2], self.max[2]),
        ]
    }
}

/// Positions on a regular grid, row-major, centred on `origin`.
pub fn grid(origin: [f32; 3], columns: u32, rows: u32, spacing: [f32; 2]) -> Result<Vec<[f32; 3]>> {
    if columns == 0 || rows == 0 {
        return Err(EngineError::Action(
            "a grid needs at least one column and one row".to_owned(),
            Some("Pass positive counts.".to_owned()),
        ));
    }
    if columns.saturating_mul(rows) > MAX_POINTS {
        return Err(too_many(columns.saturating_mul(rows)));
    }
    let width = f32::from(u16::try_from(columns - 1).unwrap_or(u16::MAX)) * spacing[0];
    let depth = f32::from(u16::try_from(rows - 1).unwrap_or(u16::MAX)) * spacing[1];
    let mut out = Vec::with_capacity((columns * rows) as usize);
    for row in 0..rows {
        for column in 0..columns {
            out.push([
                (column as f32).mul_add(spacing[0], origin[0] - width / 2.0),
                origin[1],
                (row as f32).mul_add(spacing[1], origin[2] - depth / 2.0),
            ]);
        }
    }
    Ok(out)
}

/// `count` positions scattered inside `bounds`, keeping at least `min_distance` apart.
///
/// Uses bounded-attempt dart throwing: a request that cannot be satisfied returns the
/// points it *could* place rather than looping forever, and the caller can see it got
/// fewer than it asked for. Silently overlapping props would look like a physics bug later.
pub fn scatter(bounds: Bounds, count: u32, min_distance: f32, seed: u64) -> Result<Vec<[f32; 3]>> {
    if count == 0 {
        return Err(EngineError::Action(
            "scatter needs a positive count".to_owned(),
            Some("Ask for at least one.".to_owned()),
        ));
    }
    if count > MAX_POINTS {
        return Err(too_many(count));
    }
    let mut rng = Rng::new(seed);
    let mut out: Vec<[f32; 3]> = Vec::with_capacity(count as usize);
    let attempts = count.saturating_mul(32);
    let min_sq = min_distance.max(0.0) * min_distance.max(0.0);
    for _ in 0..attempts {
        if out.len() == count as usize {
            break;
        }
        let candidate = [
            rng.range(bounds.min[0], bounds.max[0]),
            rng.range(bounds.min[1], bounds.max[1]),
            rng.range(bounds.min[2], bounds.max[2]),
        ];
        if min_sq > 0.0
            && out
                .iter()
                .any(|placed| distance_squared(*placed, candidate) < min_sq)
        {
            continue;
        }
        out.push(candidate);
    }
    Ok(out)
}

/// Positions evenly spaced around a circle on the XZ plane.
pub fn ring(center: [f32; 3], radius: f32, count: u32) -> Result<Vec<[f32; 3]>> {
    if count == 0 {
        return Err(EngineError::Action(
            "a ring needs a positive count".to_owned(),
            Some("Ask for at least one.".to_owned()),
        ));
    }
    if count > MAX_POINTS {
        return Err(too_many(count));
    }
    let step = std::f32::consts::TAU / count as f32;
    Ok((0..count)
        .map(|index| {
            let angle = step * index as f32;
            [
                radius.mul_add(angle.cos(), center[0]),
                center[1],
                radius.mul_add(angle.sin(), center[2]),
            ]
        })
        .collect())
}

/// Positions along the inside edge of `bounds`, `spacing` apart — the wall line of a room.
pub fn perimeter(bounds: Bounds, spacing: f32) -> Result<Vec<[f32; 3]>> {
    if spacing <= 0.0 || !spacing.is_finite() {
        return Err(EngineError::Action(
            "perimeter spacing must be positive".to_owned(),
            Some("Use a spacing greater than zero.".to_owned()),
        ));
    }
    let size = bounds.size();
    let columns = (size[0] / spacing).floor().max(1.0) as u32 + 1;
    let rows = (size[2] / spacing).floor().max(1.0) as u32 + 1;
    if columns.saturating_mul(rows) > MAX_POINTS {
        return Err(too_many(columns.saturating_mul(rows)));
    }
    let mut out = Vec::new();
    for column in 0..columns {
        let x = (column as f32)
            .mul_add(spacing, bounds.min[0])
            .min(bounds.max[0]);
        out.push([x, bounds.min[1], bounds.min[2]]);
        out.push([x, bounds.min[1], bounds.max[2]]);
    }
    // Skip the corners on the side runs; they are already placed above.
    for row in 1..rows.saturating_sub(1) {
        let z = (row as f32)
            .mul_add(spacing, bounds.min[2])
            .min(bounds.max[2]);
        out.push([bounds.min[0], bounds.min[1], z]);
        out.push([bounds.max[0], bounds.min[1], z]);
    }
    Ok(out)
}

/// Positions stacked straight up from `base`, `spacing` apart.
pub fn stack(base: [f32; 3], count: u32, spacing: f32) -> Result<Vec<[f32; 3]>> {
    if count == 0 {
        return Err(EngineError::Action(
            "a stack needs a positive count".to_owned(),
            Some("Ask for at least one.".to_owned()),
        ));
    }
    if count > MAX_POINTS {
        return Err(too_many(count));
    }
    Ok((0..count)
        .map(|index| [base[0], (index as f32).mul_add(spacing, base[1]), base[2]])
        .collect())
}

/// Build four oriented walls around `bounds`, splitting walls around door/window openings.
pub fn room_from_bounds(
    bounds: Bounds,
    height: f32,
    thickness: f32,
    openings: &[WallOpening],
) -> Result<Vec<Placement>> {
    validate_architecture(bounds, height, thickness)?;
    let size = bounds.size();
    if thickness * 2.0 >= size[0].min(size[2]) {
        return Err(action_error(
            "wall thickness consumes the room interior",
            "Use thinner walls or wider room bounds.",
        ));
    }

    let mut out = Vec::new();
    for wall in [
        RoomWall::North,
        RoomWall::South,
        RoomWall::East,
        RoomWall::West,
    ] {
        let length = match wall {
            RoomWall::North | RoomWall::South => size[0],
            RoomWall::East | RoomWall::West => size[2],
        };
        let mut cuts: Vec<WallOpening> = openings
            .iter()
            .copied()
            .filter(|opening| opening.wall == wall)
            .collect();
        cuts.sort_by(|a, b| {
            a.offset
                .partial_cmp(&b.offset)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        validate_openings(&cuts, length, height)?;

        let half = length / 2.0;
        let mut cursor = -half;
        for opening in cuts {
            let start = opening.offset - opening.width / 2.0;
            let end = opening.offset + opening.width / 2.0;
            push_wall_segment(
                &mut out,
                bounds,
                wall,
                cursor,
                start,
                height,
                height / 2.0,
                thickness,
            )?;
            let lintel_height = height - opening.height;
            if lintel_height > f32::EPSILON {
                push_wall_segment(
                    &mut out,
                    bounds,
                    wall,
                    start,
                    end,
                    lintel_height,
                    opening.height + lintel_height / 2.0,
                    thickness,
                )?;
            }
            cursor = end;
        }
        push_wall_segment(
            &mut out,
            bounds,
            wall,
            cursor,
            half,
            height,
            height / 2.0,
            thickness,
        )?;
    }
    enforce_placement_cap(out.len())?;
    Ok(out)
}

/// Join two non-overlapping rooms with two open-ended, oriented corridor walls.
pub fn corridor_between(
    from_room: Bounds,
    to_room: Bounds,
    width: f32,
    height: f32,
    thickness: f32,
) -> Result<CorridorLayout> {
    validate_architecture(from_room, height, thickness)?;
    validate_architecture(to_room, height, thickness)?;
    if !width.is_finite() || width <= 0.0 {
        return Err(action_error(
            "corridor width must be positive",
            "Use a finite width greater than zero.",
        ));
    }
    if overlaps_xz(from_room, to_room) {
        return Err(action_error(
            "corridor rooms overlap",
            "Use two separate room bounds.",
        ));
    }

    let from_center = from_room.center();
    let to_center = to_room.center();
    let dx = to_center[0] - from_center[0];
    let dz = to_center[2] - from_center[2];
    let distance = dx.hypot(dz);
    if !distance.is_finite() || distance <= f32::EPSILON {
        return Err(action_error(
            "corridor rooms have the same centre",
            "Use two separate room bounds.",
        ));
    }
    let direction = [dx / distance, dz / distance];
    let from = boundary_point(from_room, direction, true);
    let to = boundary_point(to_room, direction, false);
    let run_x = to[0] - from[0];
    let run_z = to[2] - from[2];
    let run = run_x.hypot(run_z);
    if run <= thickness {
        return Err(action_error(
            "corridor has no usable run between the rooms",
            "Move the rooms farther apart or use thinner walls.",
        ));
    }
    let unit = [run_x / run, run_z / run];
    let perpendicular = [-unit[1], unit[0]];
    let center = [
        f32::midpoint(from[0], to[0]),
        from_room.min[1] + height / 2.0,
        f32::midpoint(from[2], to[2]),
    ];
    let side_offset = width / 2.0 + thickness / 2.0;
    let yaw = unit[1].atan2(unit[0]);
    let placements = [-1.0_f32, 1.0]
        .into_iter()
        .map(|side| Placement {
            position: [
                side.mul_add(perpendicular[0] * side_offset, center[0]),
                center[1],
                side.mul_add(perpendicular[1] * side_offset, center[2]),
            ],
            yaw,
            scale: [run, height, thickness],
        })
        .collect::<Vec<_>>();
    enforce_placement_cap(placements.len())?;
    Ok(CorridorLayout {
        placements,
        from,
        to,
    })
}

fn validate_architecture(bounds: Bounds, height: f32, thickness: f32) -> Result<()> {
    let size = bounds.size();
    if size[0] <= 0.0 || size[2] <= 0.0 {
        return Err(action_error(
            "architectural bounds need positive width and depth",
            "Widen the bounds on X and Z.",
        ));
    }
    if !height.is_finite() || height <= 0.0 || !thickness.is_finite() || thickness <= 0.0 {
        return Err(action_error(
            "height and thickness must be positive finite numbers",
            "Use positive wall dimensions.",
        ));
    }
    Ok(())
}

fn validate_openings(openings: &[WallOpening], wall_length: f32, room_height: f32) -> Result<()> {
    let mut previous_end = -wall_length / 2.0;
    for opening in openings {
        if !opening.offset.is_finite()
            || !opening.width.is_finite()
            || !opening.height.is_finite()
            || opening.width <= 0.0
            || opening.height <= 0.0
            || opening.height > room_height
        {
            return Err(action_error(
                "opening dimensions are invalid",
                "Use a positive width and a height no taller than the room.",
            ));
        }
        let start = opening.offset - opening.width / 2.0;
        let end = opening.offset + opening.width / 2.0;
        if start < -wall_length / 2.0 || end > wall_length / 2.0 {
            return Err(action_error(
                "opening extends beyond its wall",
                "Reduce its width or move its offset toward the wall centre.",
            ));
        }
        if start < previous_end - f32::EPSILON {
            return Err(action_error(
                "wall openings overlap",
                "Separate the openings on that wall.",
            ));
        }
        previous_end = end;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn push_wall_segment(
    out: &mut Vec<Placement>,
    bounds: Bounds,
    wall: RoomWall,
    start: f32,
    end: f32,
    height: f32,
    center_y_above_floor: f32,
    thickness: f32,
) -> Result<()> {
    let length = end - start;
    if length <= f32::EPSILON {
        return Ok(());
    }
    enforce_placement_cap(out.len() + 1)?;
    let along = f32::midpoint(start, end);
    let floor = bounds.min[1];
    let (position, yaw) = match wall {
        RoomWall::North => ([bounds.center()[0] + along, floor, bounds.max[2]], 0.0),
        RoomWall::South => ([bounds.center()[0] + along, floor, bounds.min[2]], 0.0),
        RoomWall::East => (
            [bounds.max[0], floor, bounds.center()[2] + along],
            std::f32::consts::FRAC_PI_2,
        ),
        RoomWall::West => (
            [bounds.min[0], floor, bounds.center()[2] + along],
            std::f32::consts::FRAC_PI_2,
        ),
    };
    out.push(Placement {
        position: [position[0], floor + center_y_above_floor, position[2]],
        yaw,
        scale: [length, height, thickness],
    });
    Ok(())
}

fn overlaps_xz(a: Bounds, b: Bounds) -> bool {
    a.min[0] < b.max[0] && a.max[0] > b.min[0] && a.min[2] < b.max[2] && a.max[2] > b.min[2]
}

fn boundary_point(bounds: Bounds, direction: [f32; 2], forward: bool) -> [f32; 3] {
    let direction = if forward {
        direction
    } else {
        [-direction[0], -direction[1]]
    };
    let center = bounds.center();
    let tx = if direction[0].abs() <= f32::EPSILON {
        f32::INFINITY
    } else if direction[0] > 0.0 {
        (bounds.max[0] - center[0]) / direction[0]
    } else {
        (bounds.min[0] - center[0]) / direction[0]
    };
    let tz = if direction[1].abs() <= f32::EPSILON {
        f32::INFINITY
    } else if direction[1] > 0.0 {
        (bounds.max[2] - center[2]) / direction[1]
    } else {
        (bounds.min[2] - center[2]) / direction[1]
    };
    let travel = tx.min(tz);
    [
        direction[0].mul_add(travel, center[0]),
        bounds.min[1],
        direction[1].mul_add(travel, center[2]),
    ]
}

fn enforce_placement_cap(count: usize) -> Result<()> {
    if count > MAX_POINTS as usize {
        return Err(too_many(u32::try_from(count).unwrap_or(u32::MAX)));
    }
    Ok(())
}

fn action_error(message: &str, hint: &str) -> EngineError {
    EngineError::Action(message.to_owned(), Some(hint.to_owned()))
}

/// The ceiling on how many things one call may place.
///
/// A model that asks for a million crates has made a mistake, and finding out by watching
/// the editor lock up is the worst way to learn it.
pub const MAX_POINTS: u32 = 4096;

fn too_many(requested: u32) -> EngineError {
    EngineError::Action(
        format!("{requested} placements exceeds the {MAX_POINTS} limit for one call"),
        Some("Split it into several calls, or use a coarser spacing.".to_owned()),
    )
}

fn distance_squared(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    dx.mul_add(dx, dy.mul_add(dy, dz * dz))
}

#[cfg(test)]
mod tests {
    use super::{
        corridor_between, grid, perimeter, ring, room_from_bounds, scatter, stack, Bounds, Rng,
        RoomWall, WallOpening, MAX_POINTS,
    };

    fn bounds() -> Bounds {
        Bounds::new([-10.0, 0.0, -10.0], [10.0, 0.0, 10.0]).expect("valid bounds")
    }

    #[test]
    fn the_same_seed_always_produces_the_same_layout() {
        let first = scatter(bounds(), 32, 1.0, 42).expect("scatter");
        let second = scatter(bounds(), 32, 1.0, 42).expect("scatter");
        assert_eq!(first, second, "a generated level must be reproducible");

        let different = scatter(bounds(), 32, 1.0, 43).expect("scatter");
        assert_ne!(first, different, "a different seed is a different layout");
    }

    #[test]
    fn scatter_respects_the_bounds_and_the_minimum_distance() {
        let area = bounds();
        let points = scatter(area, 24, 2.0, 7).expect("scatter");
        for point in &points {
            for (axis, value) in point.iter().enumerate() {
                assert!(*value >= area.min[axis] && *value <= area.max[axis]);
            }
        }
        for (index, a) in points.iter().enumerate() {
            for b in points.iter().skip(index + 1) {
                let gap = super::distance_squared(*a, *b).sqrt();
                assert!(gap >= 2.0 - 1e-4, "points {gap} apart, wanted >= 2");
            }
        }
    }

    #[test]
    fn an_impossible_scatter_returns_what_fits_instead_of_hanging() {
        // 200 points two metres apart cannot fit in a 4×4 area. The call must return.
        let tight = Bounds::new([0.0, 0.0, 0.0], [4.0, 0.0, 4.0]).expect("bounds");
        let points = scatter(tight, 200, 2.0, 1).expect("returns rather than looping");
        assert!(!points.is_empty());
        assert!(
            points.len() < 200,
            "the caller can see it placed fewer than asked"
        );
    }

    #[test]
    fn a_grid_is_centred_on_its_origin_and_row_major() {
        let points = grid([0.0, 0.0, 0.0], 3, 2, [2.0, 4.0]).expect("grid");
        assert_eq!(points.len(), 6);
        assert_eq!(points[0], [-2.0, 0.0, -2.0]);
        assert_eq!(points[2], [2.0, 0.0, -2.0]);
        assert_eq!(points[3], [-2.0, 0.0, 2.0], "row-major order");
        let mean_x: f32 = points.iter().map(|point| point[0]).sum::<f32>() / 6.0;
        assert!(mean_x.abs() < 1e-5, "centred on the origin");
    }

    #[test]
    fn a_ring_is_evenly_spaced_at_the_given_radius() {
        let points = ring([1.0, 2.0, 3.0], 5.0, 8).expect("ring");
        assert_eq!(points.len(), 8);
        for point in &points {
            let dx = point[0] - 1.0;
            let dz = point[2] - 3.0;
            assert!((dx.hypot(dz) - 5.0).abs() < 1e-4);
            assert_eq!(point[1], 2.0, "a ring stays at the centre's height");
        }
    }

    #[test]
    fn a_perimeter_traces_the_edge_and_never_the_middle() {
        let area = bounds();
        let points = perimeter(area, 5.0).expect("perimeter");
        assert!(!points.is_empty());
        for point in &points {
            let on_x_edge =
                (point[0] - area.min[0]).abs() < 1e-3 || (point[0] - area.max[0]).abs() < 1e-3;
            let on_z_edge =
                (point[2] - area.min[2]).abs() < 1e-3 || (point[2] - area.max[2]).abs() < 1e-3;
            assert!(on_x_edge || on_z_edge, "{point:?} is not on the edge");
        }
    }

    #[test]
    fn a_stack_goes_straight_up() {
        let points = stack([1.0, 0.5, 2.0], 4, 1.0).expect("stack");
        assert_eq!(points.len(), 4);
        assert_eq!(points[3], [1.0, 3.5, 2.0]);
    }

    #[test]
    fn absurd_counts_are_refused_before_anything_is_built() {
        let error = scatter(bounds(), MAX_POINTS + 1, 0.0, 1).expect_err("too many");
        assert!(error.hint().is_some_and(|hint| hint.contains("Split")));
        assert!(grid([0.0, 0.0, 0.0], 4096, 4096, [1.0, 1.0]).is_err());
        assert!(ring([0.0, 0.0, 0.0], 1.0, 0).is_err());
        assert!(stack([0.0, 0.0, 0.0], 0, 1.0).is_err());
    }

    #[test]
    fn inverted_bounds_are_rejected() {
        let error = Bounds::new([5.0, 0.0, 0.0], [-5.0, 0.0, 0.0]).expect_err("inverted");
        assert!(error.hint().is_some());
    }

    #[test]
    fn the_rng_stays_inside_its_declared_ranges() {
        let mut rng = Rng::new(0);
        for _ in 0..2_000 {
            let unit = rng.unit();
            assert!((0.0..1.0).contains(&unit), "unit out of range: {unit}");
            let ranged = rng.range(-3.0, 7.0);
            assert!((-3.0..7.0).contains(&ranged));
        }
        // An empty range is a value, not a panic.
        assert_eq!(rng.range(2.0, 2.0), 2.0);
    }

    #[test]
    fn room_opening_splits_a_wall_and_preserves_a_lintel() {
        let room = Bounds::new([-5.0, 0.0, -4.0], [5.0, 3.0, 4.0]).expect("bounds");
        let placements = room_from_bounds(
            room,
            3.0,
            0.2,
            &[WallOpening {
                wall: RoomWall::North,
                offset: 0.0,
                width: 2.0,
                height: 2.2,
            }],
        )
        .expect("room");
        assert_eq!(
            placements.len(),
            6,
            "three north pieces plus three intact walls"
        );
        assert!(placements.iter().any(|placement| {
            (placement.position[2] - 4.0).abs() < 1e-5
                && (placement.scale[0] - 2.0).abs() < 1e-5
                && (placement.scale[1] - 0.8).abs() < 1e-5
        }));
        assert!(placements
            .iter()
            .any(|placement| (placement.yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-5));
    }

    #[test]
    fn invalid_and_overlapping_openings_are_rejected() {
        let room = Bounds::new([-5.0, 0.0, -4.0], [5.0, 0.0, 4.0]).expect("bounds");
        let overlap = [
            WallOpening {
                wall: RoomWall::South,
                offset: -0.5,
                width: 2.0,
                height: 2.0,
            },
            WallOpening {
                wall: RoomWall::South,
                offset: 0.5,
                width: 2.0,
                height: 2.0,
            },
        ];
        assert!(room_from_bounds(room, 3.0, 0.2, &overlap).is_err());
        assert!(room_from_bounds(
            room,
            3.0,
            0.2,
            &[WallOpening {
                wall: RoomWall::East,
                offset: 20.0,
                width: 1.0,
                height: 2.0,
            }]
        )
        .is_err());
    }

    #[test]
    fn corridor_endpoints_touch_both_rooms_and_walls_are_oriented() {
        let a = Bounds::new([-6.0, 0.0, -3.0], [-2.0, 0.0, 3.0]).expect("a");
        let b = Bounds::new([4.0, 0.0, 1.0], [8.0, 0.0, 5.0]).expect("b");
        let corridor = corridor_between(a, b, 2.0, 3.0, 0.2).expect("corridor");
        assert_eq!(corridor.placements.len(), 2);
        assert!((corridor.from[0] - a.max[0]).abs() < 1e-5);
        assert!(
            (corridor.to[0] - b.min[0]).abs() < 1e-5 || (corridor.to[2] - b.min[2]).abs() < 1e-5
        );
        assert!(corridor.placements[0].yaw.abs() > 0.01);
    }

    #[test]
    fn overlapping_rooms_cannot_have_a_corridor() {
        let a = Bounds::new([-2.0, 0.0, -2.0], [2.0, 0.0, 2.0]).expect("a");
        let b = Bounds::new([1.0, 0.0, 1.0], [4.0, 0.0, 4.0]).expect("b");
        assert!(corridor_between(a, b, 2.0, 3.0, 0.2).is_err());
    }
}
