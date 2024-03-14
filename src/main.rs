// @author:    olinex
// @time:      2023/12/27

// self mods

// use other mods
use clap::Parser;
use frontier_fs::block::{BlockDevice, BLOCK_DEVICE_REGISTER};
use frontier_fs::configs::BLOCK_BYTE_SIZE;
use frontier_fs::vfs::{FileFlags, FileSystem, InitMode, FS};
use std::boxed::Box;
use std::fs::{read_dir, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::Mutex;

// use self mods

const IABC: u8 = 4;
const MAX_IMAGE_BLOCKS: u32 = 1024;

enum RawDeviceErrorCode {
    Locked = -1,
}

/// Impl std file as frontier file system's block device,
/// so we can read/write data to file as block device
struct BlockFile(Mutex<File>);
impl BlockDevice for BlockFile {
    fn read_block(&self, id: usize, buffer: &mut [u8]) -> Option<isize> {
        match self.0.lock() {
            Ok(mut file) => {
                let seek = SeekFrom::Start((id * BLOCK_BYTE_SIZE) as u64);
                file.seek(seek).expect("Error when seeking!");
                file.read(buffer).unwrap();
                None
            }
            Err(_) => Some(RawDeviceErrorCode::Locked as isize),
        }
    }

    fn write_block(&self, id: usize, buffer: &[u8]) -> Option<isize> {
        match self.0.lock() {
            Ok(mut file) => {
                file.seek(SeekFrom::Start((id * BLOCK_BYTE_SIZE) as u64))
                    .expect("Error when seeking!");
                assert_eq!(
                    file.write(buffer).unwrap(),
                    BLOCK_BYTE_SIZE,
                    "Not a complete block!"
                );
                None
            }
            Err(_) => Some(RawDeviceErrorCode::Locked as isize),
        }
    }
}

/// Define the optional parameters for the image file build command
#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Executable source dir
    #[arg(short, long)]
    source_dir: PathBuf,

    /// Executable target file path
    #[arg(long)]
    target_path: PathBuf,

    /// Check target file validity after packing
    #[arg(short, long)]
    check: bool,
}

/// Build the image file
fn build(args: Args) {
    assert!(args.source_dir.exists() && args.source_dir.is_dir());
    assert!(args.target_path.parent().is_some_and(|dir| dir.is_dir()));
    let block_file: Box<dyn BlockDevice> = Box::new(BlockFile(Mutex::new({
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&args.target_path)
            .unwrap()
    })));
    let tracker = BLOCK_DEVICE_REGISTER.lock().mount(block_file).unwrap();
    let file_paths: Vec<_> = read_dir(args.source_dir)
        .unwrap()
        .into_iter()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file())
        .collect();
    let fs = FS::initialize(InitMode::TotalBlocks(MAX_IMAGE_BLOCKS), IABC, &tracker).unwrap();
    let root_inode = fs.root_inode();
    let flags = FileFlags::RWX;
    let mut buffer = [0; BLOCK_BYTE_SIZE];
    for file_path in file_paths.iter() {
        let name = file_path.file_name().unwrap().to_str().unwrap();
        let mut file = OpenOptions::new().read(true).open(file_path).unwrap();
        let child_inode = root_inode.create_child_inode(name, flags).unwrap();
        let new_byte_size = file.metadata().unwrap().len();
        child_inode.to_byte_size(new_byte_size).unwrap();
        let mut start_offset = 0;
        loop {
            match file.read(&mut buffer).unwrap() {
                0 => break,
                rsize => {
                    let wsize = child_inode
                        .write_buffer(&buffer[..rsize], start_offset)
                        .unwrap();
                    assert_eq!(rsize, wsize);
                    start_offset += rsize as u64;
                }
            }
        }
        assert_eq!(new_byte_size, start_offset);
        println!(
            "loaded {} file from {}({} bytes)",
            name,
            file_path.to_str().unwrap(),
            start_offset
        );
    }
    if args.check {
        let fs = FS::open(&tracker).unwrap();
        let root_inode = fs.root_inode();
        for file_path in file_paths.iter() {
            let name = file_path.file_name().unwrap().to_str().unwrap();
            let mut file = OpenOptions::new().read(true).open(file_path).unwrap();
            let child_inode = root_inode.get_child_inode(name).unwrap().unwrap();
            let new_data = child_inode.read_all().unwrap();
            let mut old_data = vec![];
            file.read_to_end(&mut old_data).unwrap();
            assert_eq!(new_data.len(), old_data.len());
        }
        println!(
            "block file {} is valid!",
            args.target_path.to_str().unwrap()
        );
    };
}

fn main() {
    build(Args::parse());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_main() {
        let args = Args {
            source_dir: PathBuf::from_str(
                "../frontier_user/target/riscv64gc-unknown-none-elf/release/images",
            )
            .unwrap(),
            target_path: PathBuf::from_str(
                "../frontier_user/target/riscv64gc-unknown-none-elf/release/user-fs.img",
            )
            .unwrap(),
            check: true,
        };
        build(args);
    }
}
