use std::collections::{HashMap, HashSet};

fn pixel_index(width: u32, x: u32, y: u32) -> Option<usize> {
    let offset = y.checked_mul(width)?.checked_add(x)?;
    usize::try_from(offset).ok()
}

fn is_filled(width: u32, pixels: &[u8], point: (u32, u32)) -> bool {
    pixel_index(width, point.0, point.1)
        .and_then(|index| pixels.get(index))
        .is_some_and(|pixel| *pixel != 0)
}

pub fn collect_pixel_rects(
    width: u32,
    height: u32,
    pixels: &[u8],
    merge_rects: bool,
) -> Vec<(u32, u32, u32, u32)> {
    if !merge_rects {
        return collect_single_pixel_rects(width, height, pixels);
    }

    collect_merged_rects((width, height), pixels)
}

fn collect_single_pixel_rects(width: u32, height: u32, pixels: &[u8]) -> Vec<(u32, u32, u32, u32)> {
    let mut rects = Vec::new();
    for y in 0..height {
        for x in 0..width {
            if is_filled(width, pixels, (x, y)) {
                rects.push((x, y, 1, 1));
            }
        }
    }
    rects
}

fn collect_merged_rects(dimensions: (u32, u32), pixels: &[u8]) -> Vec<(u32, u32, u32, u32)> {
    let (width, height) = dimensions;
    let mut active: HashMap<(u32, u32), (u32, u32)> = HashMap::new();
    let mut rects = Vec::new();

    for y in 0..height {
        let mut seen = HashSet::new();
        for run in collect_row_runs(width, pixels, y) {
            seen.insert(run);
            if let Some((_, run_h)) = active.get_mut(&run) {
                *run_h = run_h.saturating_add(1);
            } else {
                active.insert(run, (y, 1));
            }
        }
        close_stale_runs(&mut active, &seen, &mut rects);
    }

    for (key, (start_y, run_h)) in active {
        rects.push((key.0, start_y, key.1, run_h));
    }

    rects
}

fn collect_row_runs(width: u32, pixels: &[u8], y: u32) -> Vec<(u32, u32)> {
    let mut runs = Vec::new();
    let mut x = 0_u32;
    while x < width {
        if !is_filled(width, pixels, (x, y)) {
            x = x.saturating_add(1);
            continue;
        }
        let start = x;
        x = x.saturating_add(1);
        while x < width && is_filled(width, pixels, (x, y)) {
            x = x.saturating_add(1);
        }
        runs.push((start, x.saturating_sub(start)));
    }
    runs
}

fn close_stale_runs(
    active: &mut HashMap<(u32, u32), (u32, u32)>,
    seen: &HashSet<(u32, u32)>,
    rects: &mut Vec<(u32, u32, u32, u32)>,
) {
    let stale: Vec<_> = active
        .keys()
        .filter(|run| !seen.contains(*run))
        .copied()
        .collect();
    for run in stale {
        if let Some((start_y, height)) = active.remove(&run) {
            rects.push((run.0, start_y, run.1, height));
        }
    }
}
