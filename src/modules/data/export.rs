//! Writes the computed layout to a file (FR7).

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

use bevy::math::Vec3;

/// Saves the positions as CSV, one line per vertex: `id,x,y,z`.
pub fn save_layout(path: &Path, positions: &[Vec3]) -> io::Result<()> {
    let mut file = BufWriter::new(File::create(path)?);
    write_layout(&mut file, positions)?;
    file.flush()
}

fn write_layout(out: &mut impl Write, positions: &[Vec3]) -> io::Result<()> {
    writeln!(out, "id,x,y,z")?;

    for (id, position) in positions.iter().enumerate() {
        writeln!(out, "{id},{},{},{}", position.x, position.y, position.z)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_header_and_one_line_per_vertex() {
        let mut out = Vec::new();
        write_layout(&mut out, &[Vec3::new(1.5, -2.0, 0.0), Vec3::ONE]).unwrap();

        assert_eq!(String::from_utf8(out).unwrap(), "id,x,y,z\n0,1.5,-2,0\n1,1,1,1\n");
    }
}
