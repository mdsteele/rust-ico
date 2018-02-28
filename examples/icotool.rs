extern crate ico;
extern crate clap;

use clap::{App, Arg, SubCommand};
use std::fs;

// ========================================================================= //

fn main() {
    let matches = App::new("icotool")
        .version("0.1")
        .author("Matthew D. Steele <mdsteele@alum.mit.edu>")
        .about("Manipulates ICO files")
        .subcommand(SubCommand::with_name("list")
                        .about("Lists icons in an ICO file")
                        .arg(Arg::with_name("ico").required(true)))
        .get_matches();
    if let Some(submatches) = matches.subcommand_matches("list") {
        let path = submatches.value_of("ico").unwrap();
        let file = fs::File::open(path).unwrap();
        let icondir = ico::IconDir::read(file).unwrap();
        println!("There are {} {:?} entries",
                 icondir.entries().len(),
                 icondir.resource_type());
        for entry in icondir.entries() {
            println!("{}x{}", entry.width(), entry.height());
        }
    }
}

// ========================================================================= //
