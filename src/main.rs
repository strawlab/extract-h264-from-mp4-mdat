//! extract the first MP4 mdat box as an H264 file
//!
//! This utility program arose to rescue mp4 files which were not properly
//! terminated. This program makes several assumptions that are not checked and
//! could easily be wrong. Use with care.
//!
//! Usage:
//!     mp4dump INFILENAME OUTFILENAME
use std::{
    convert::TryInto,
    env,
    fs::File,
    io::{prelude::*, BufReader, BufWriter, SeekFrom},
};

// use dbg_hex::dbg_hex;
use eyre::Context;
use mp4::{skip_box, BoxHeader, BoxType};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

fn main() -> eyre::Result<()> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        println!("Usage: mp4dump INFILENAME OUTFILENAME");
        std::process::exit(1);
    }
    let infilename = &args[1];
    let outfilename = &args[2];

    let f = File::open(&infilename).with_context(|| format!("while opening {infilename}"))?;
    let out_fd =
        File::create_new(&outfilename).with_context(|| format!("while creating {outfilename}"))?;
    let size = f.metadata()?.len();
    let mut reader = BufReader::new(f);

    let mut wtr = BufWriter::new(out_fd);

    let start = reader.stream_position()?;

    let mut current = start;
    while current < size {
        // Get box header.
        dbg_hex::dbg_hex!(current);
        let header = dbg!(BoxHeader::read(&mut reader)?);
        let BoxHeader { name, size: s } = header;
        dbg_hex::dbg_hex!(s, current + s);
        if s > size {
            eyre::bail!("file contains a box with a larger size than it");
        }

        if name == BoxType::MdatBox {
            println!("mdat header starts at 0x{current:08x}");
            let mdata_data_small_start = reader.stream_position()?;
            let mdata_data_large_start = mdata_data_small_start + 8;
            println!("mdat data starts at pos 0x{mdata_data_small_start:08x}");
            println!(
                "if mdat has size > u32::MAX, data starts at pos 0x{mdata_data_large_start:08x}"
            );

            println!("u32::MAX  0x{val:08x}", val = u32::MAX);
            println!("file size 0x{size:08x}",);

            reader.seek(SeekFrom::Start(mdata_data_small_start))?;
            println!("reading as small data");
            read_nal_units(
                &mut reader,
                &mut wtr,
                (size - mdata_data_small_start).try_into().unwrap(),
            )?;
        } else {
            dbg!(1);
            skip_box(&mut reader, s)?;
        }

        // Break if size zero BoxHeader, which can result in dead-loop.
        if s == 0 {
            break;
        }

        current = reader.stream_position()?;
    }
    Ok(())
}

fn read_nal_units<R: Read + Seek, W: Write>(
    mut reader: R,
    mut wtr: W,
    size: usize,
) -> eyre::Result<usize> {
    // for nal_idx in 0..10 {
    // dbg!(nal_idx);
    let mut read_bytes = 0;
    while read_bytes + 4 < size {
        let mut buf = [0u8; 4]; // 4 bytes. See https://stackoverflow.com/a/5596471/1633026
        reader.read_exact(&mut buf).with_context(|| {
            format!("while reading header (read_bytes: {read_bytes}, size {size})")
        })?;
        read_bytes += 4;

        // dbg_hex!(buf);
        let nalu_size = u32::from_be_bytes(buf) as usize;
        let bytes_remaining = size - read_bytes;
        if nalu_size > bytes_remaining {
            tracing::warn!("Premature end of NAL unit. Stopping early.");
            break;
        }

        let mut nalu_bytes = vec![0u8; nalu_size];
        reader.read_exact(&mut nalu_bytes)?;
        read_bytes += nalu_size;

        // let show_size = nalu_size.min(8);
        // dbg_hex!(&nalu_bytes[0..show_size]);

        const START_CODE: &[u8] = &[0x00, 0x00, 0x00, 0x01];
        wtr.write_all(START_CODE)?;
        wtr.write_all(&nalu_bytes)?;
    }

    Ok(read_bytes)
}
