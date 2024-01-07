// @author:    olinex
// @time:      2023/12/27

// self mods

// use other mods
use clap::Parser;
use frontier_fs::block::BlockDevice;
use frontier_fs::configs::BLOCK_BYTE_SIZE;
use frontier_fs::vfs::{FileFlags, FileSystem, InitMode, FS};
use std::fs::{read_dir, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

// use self mods

const IABC: u8 = 4;
const MAX_IMAGE_BLOCKS: u32 = 1024;

enum RawDeviceErrorCode {
    Locked = -1,
}

struct BlockFile(Mutex<File>);
impl BlockDevice for BlockFile {
    fn read_block(&self, id: usize, buffer: &mut [u8]) -> Option<isize> {
        match self.0.lock() {
            Ok(mut file) => {
                file.seek(SeekFrom::Start((id * BLOCK_BYTE_SIZE) as u64))
                    .expect("Error when seeking!");
                assert_eq!(
                    file.read(buffer).unwrap(),
                    BLOCK_BYTE_SIZE,
                    "Not a complete block!"
                );
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

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Executable source dir
    #[arg(short, long)]
    source_dir: PathBuf,

    /// Executable target dir
    #[arg(long)]
    target_dir: PathBuf,

    /// Target file name
    #[arg(long)]
    traget_file_name: String,
}

fn main() {
    let args = Args::parse();
    let block_file: Arc<dyn BlockDevice> = Arc::new(BlockFile(Mutex::new({
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(args.target_dir.join(args.traget_file_name))
            .unwrap()
    })));
    let file_paths: Vec<_> = read_dir(args.source_dir)
        .unwrap()
        .into_iter()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file())
        .collect();
    let fs = FS::initialize(InitMode::TotalBlocks(MAX_IMAGE_BLOCKS), IABC, &block_file).unwrap();
    let root_inode = fs.root_inode();
    let flags = FileFlags::VALID;
    let mut mfs = fs.lock();
    let mut buffer = [0; BLOCK_BYTE_SIZE];
    let device = Arc::clone(mfs.device());
    for file_path in file_paths.iter() {
        let name = file_path.file_name().unwrap().to_str().unwrap();
        let mut file = OpenOptions::new().read(true).open(file_path).unwrap();
        let mut start_offset = 0;
        let child_inode = root_inode
            .create_child_inode(name, flags, &mut mfs)
            .unwrap();
        child_inode
            .modify_disk_inode(&device, |disk_inode| loop {
                match file.read(&mut buffer).unwrap() {
                    0 => break,
                    rsize => {
                        let wsize = disk_inode
                            .write_at(start_offset, &buffer[0..rsize], &device)
                            .unwrap();
                        assert_eq!(rsize, wsize as usize);
                        start_offset += rsize as u64;
                    }
                }
            })
            .unwrap();
        print!(
            "loaded {} file({} bytes)",
            file_path.to_str().unwrap(),
            start_offset
        )
    }
}
