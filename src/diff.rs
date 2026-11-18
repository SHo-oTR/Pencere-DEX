pub const TILE_SIZE: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x0: usize,
    pub y0: usize,
    pub x1: usize,
    pub y1: usize,
}

impl Rect {
    pub fn width(&self) -> usize {
        self.x1.saturating_sub(self.x0)
    }

    pub fn height(&self) -> usize {
        self.y1.saturating_sub(self.y0)
    }
}

/// Computes changed tile regions between two frames of dimensions (width x height)
/// Returns (list of bounding boxes, fraction of changed tiles)
pub fn compute_diff(
    prev: &[u32],
    cur: &[u32],
    width: usize,
    height: usize,
) -> (Vec<Rect>, f32) {
    if prev.is_empty() || prev.len() != cur.len() {
        return (vec![Rect { x0: 0, y0: 0, x1: width, y1: height }], 1.0);
    }

    let t = TILE_SIZE;
    let tiles_x = (width + t - 1) / t;
    let tiles_y = (height + t - 1) / t;
    let total_tiles = tiles_x * tiles_y;

    if total_tiles == 0 {
        return (Vec::new(), 0.0);
    }

    let mut dirty_tiles = vec![false; total_tiles];
    let mut dirty_count = 0;

    for ty in 0..tiles_y {
        let y_start = ty * t;
        let y_end = (y_start + t).min(height);

        for tx in 0..tiles_x {
            let x_start = tx * t;
            let x_end = (x_start + t).min(width);

            let mut tile_dirty = false;
            'outer: for y in y_start..y_end {
                let row_offset = y * width;
                for x in x_start..x_end {
                    let idx = row_offset + x;
                    // Compare RGB only (mask off alpha byte if necessary)
                    if (prev[idx] & 0x00FFFFFF) != (cur[idx] & 0x00FFFFFF) {
                        tile_dirty = true;
                        break 'outer;
                    }
                }
            }

            if tile_dirty {
                dirty_tiles[ty * tiles_x + tx] = true;
                dirty_count += 1;
            }
        }
    }

    let dirty_fraction = dirty_count as f32 / total_tiles as f32;
    if dirty_count == 0 {
        return (Vec::new(), 0.0);
    }

    // If more than 50% of the screen changed, refresh whole frame
    if dirty_fraction > 0.5 {
        return (vec![Rect { x0: 0, y0: 0, x1: width, y1: height }], dirty_fraction);
    }

    // Find rows that have any dirty tiles
    let mut row_has_dirty = vec![false; tiles_y];
    for ty in 0..tiles_y {
        for tx in 0..tiles_x {
            if dirty_tiles[ty * tiles_x + tx] {
                row_has_dirty[ty] = true;
                break;
            }
        }
    }

    // Merge adjacent dirty rows into bands (allowing small 1-2 row gaps)
    let mut bands: Vec<(usize, usize)> = Vec::new();
    let mut start: Option<usize> = None;
    let mut last = 0;

    for (ty, &has_dirty) in row_has_dirty.iter().enumerate() {
        if has_dirty {
            match start {
                None => {
                    start = Some(ty);
                    last = ty;
                }
                Some(_) => {
                    if ty - last > 2 {
                        bands.push((start.unwrap(), last));
                        start = Some(ty);
                    }
                    last = ty;
                }
            }
        }
    }
    if let Some(s) = start {
        bands.push((s, last));
    }

    // Limit maximum band count to avoid too many small network chunks
    if bands.len() > 6 {
        let first = bands.first().unwrap().0;
        let final_last = bands.last().unwrap().1;
        bands = vec![(first, final_last)];
    }

    // Build bounding boxes for each band
    let mut rects = Vec::new();
    for (s_row, e_row) in bands {
        let mut min_tx = tiles_x;
        let mut max_tx = 0;

        for ty in s_row..=e_row {
            for tx in 0..tiles_x {
                if dirty_tiles[ty * tiles_x + tx] {
                    min_tx = min_tx.min(tx);
                    max_tx = max_tx.max(tx);
                }
            }
        }

        if min_tx <= max_tx {
            let x0 = min_tx * t;
            let y0 = s_row * t;
            let x1 = ((max_tx + 1) * t).min(width);
            let y1 = ((e_row + 1) * t).min(height);
            rects.push(Rect { x0, y0, x1, y1 });
        }
    }

    (rects, dirty_fraction)
}
