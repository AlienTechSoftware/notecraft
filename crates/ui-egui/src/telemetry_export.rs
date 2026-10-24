use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::Path;

pub fn export(session: &Path, destination: &Path) -> io::Result<()> {
    let mut zip = zip::ZipWriter::new(File::create(destination)?);
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for entry in fs::read_dir(session)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let mut contents = Vec::new();
        File::open(entry.path())?.read_to_end(&mut contents)?;
        zip.start_file(name, options).map_err(io::Error::other)?;
        zip.write_all(&contents)?;
    }
    zip.finish().map_err(io::Error::other)?;
    let mut signature = [0_u8; 2];
    File::open(destination)?.read_exact(&mut signature)?;
    if signature != [0x50, 0x4b] {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid ZIP export"));
    }
    fs::remove_dir_all(session)
}
