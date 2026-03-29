use std::collections::{HashMap, HashSet};

fn pixel_index(width: u32, x: u32, y: u32) -> Option<usize> {
    let offset = y.checked_mul(width)?.checked_add(x)?;
    usize::try_from(offset).ok()
}

pub fn collect_pixel_rects(
    width: u32,
    height: u32,
    pixels: &[u8],
    merge_rects: bool,
) -> Vec<(u32, u32, u32, u32)> {
    if !merge_rects {
        let mut rects = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let filled = pixel_index(width, x, y)
                    .and_then(|idx| pixels.get(idx))
                    .copied()
                    .unwrap_or(0)
                    != 0;
                if filled {
                    rects.push((x, y, 1, 1));
                }
            }
        }
        return rects;
    }

    let mut active: HashMap<(u32, u32), (u32, u32)> = HashMap::new();
    let mut rects = Vec::new();

    for y in 0..height {
        let mut runs = Vec::new();
        let mut x = 0_u32;
        while x < width {
            let filled = pixel_index(width, x, y)
                .and_then(|idx| pixels.get(idx))
                .copied()
                .unwrap_or(0)
                != 0;
            if !filled {
                x = x.saturating_add(1);
                continue;
            }

            let start = x;
            x = x.saturating_add(1);
            while x < width {
                let filled = pixel_index(width, x, y)
                    .and_then(|idx| pixels.get(idx))
                    .copied()
                    .unwrap_or(0)
                    != 0;
                if !filled {
                    break;
                }
                x = x.saturating_add(1);
            }
            runs.push((start, x.saturating_sub(start)));
        }

        let mut seen = HashSet::new();
        for run in runs {
            seen.insert(run);
            if let Some((_, run_h)) = active.get_mut(&run) {
                *run_h = run_h.saturating_add(1);
            } else {
                active.insert(run, (y, 1));
            }
        }

        let stale = active
            .keys()
            .filter(|key| !seen.contains(*key))
            .copied()
            .collect::<Vec<_>>();
        for key in stale {
            if let Some((start_y, run_h)) = active.remove(&key) {
                rects.push((key.0, start_y, key.1, run_h));
            }
        }
    }

    for (key, (start_y, run_h)) in active {
        rects.push((key.0, start_y, key.1, run_h));
    }

    rects
}
