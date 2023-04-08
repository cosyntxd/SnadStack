// Uses a few kilobytes of binary space to construct a lookup table for Bresenham's line algorithm
// The equation to find index is too slow, so another lookup table is used to find indicies
const TABLE: ([usize; 169], [isize; 1794]) = generate_table();

// Calculates the manhattan distance to the center of the grid
const fn center_dist(x: isize, y: isize) -> usize {
    let distance_x = (x - 7).abs() as usize;
    let distance_y = (y - 7).abs() as usize;

    if distance_x > distance_y {
        distance_x
    } else {
        distance_y
    }
}
// Algorithm implemenation from: https://en.wikipedia.org/wiki/Bresenham%27s_line_algorithm
const fn bresenham_line(x: isize, y: isize) -> [isize; 18] {
    let mut result = [0; 18];
    let mut index = 0;
    let dx = x.abs();
    let dy = -y.abs();
    let sx = if 0 < x { 1 } else { -1 };
    let sy = if 0 < y { 1 } else { -1 };
    let mut error = dx + dy;

    let mut x0 = 0;
    let mut y0 = 0;
    loop {
        result[index] = x0;
        result[index + 1] = y0;
        index += 2;
        if x0 == x && y0 == y {
            break;
        };
        let error_2 = 2 * error;
        if error_2 >= dy {
            if x0 == x {
                break;
            }
            error += dy;
            x0 += sx;
        }
        if error_2 <= dx {
            if y0 == y {
                break;
            }
            error += dx;
            y0 += sy;
        }
    }
    result[index] = x - 7;
    result[index + 1] = y - 7;

    result
}

const fn generate_table() -> ([usize; 169], [isize; 1794]) {
    let mut y: isize = 1;
    let mut index = 0;
    let mut indicies = [0; 13 * 13];
    let mut values = [0; 1794];
    while y < 14 {
        let mut x: isize = 1;
        while x < 14 {
            let distance = center_dist(x, y);
            let line = bresenham_line(x - 7, y - 7);
            let mut count = 0;
            while count < distance as usize * 2 + 2 {
                values[count + index] = line[count];
                count += 1;
            }
            indicies[((y - 1) * 13 + (x - 1)) as usize] = index;
            index += count;
            x += 1;
        }
        y += 1;
    }
    (indicies, values)
}
// The application calls this function to perform the lookup
pub fn line(x: isize, y: isize) -> &'static [isize] {
    // Converts coordinates to an index into the first lookup table
    let i = ((y - 1) * 13 + (x - 1)) as usize;
    // Finds the index into the array
    let j = TABLE.0[i];
    // Gets the length needed to be read
    let d = center_dist(x, y) * 2 + 2;
    &TABLE.1[j..j + d]
}
