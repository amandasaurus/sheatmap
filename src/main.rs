extern crate rstar;
extern crate csv;
extern crate clap;
#[macro_use] extern crate anyhow;
#[macro_use] extern crate log;
extern crate env_logger;
extern crate flate2;
use clap::{Arg, App};
use std::io::{Read, Write, stdin, stdout};
use std::fs::*;
use flate2::write::GzEncoder;
use flate2::Compression;

use anyhow::{Context, Result};

static EARTH_RADIUS_M: f64 = 6_371_000.;


fn main() -> Result<()> {
    let matches = App::new("sHeatMap")
        .version(env!("CARGO_PKG_VERSION"))
        .setting(clap::AppSettings::AllowLeadingHyphen)
        .about("Create heatmaps from input CSV files")
        .arg(Arg::with_name("input")
             .short("i").long("input")
             .help("Input CSV file (- to read from stdin)").value_name("INPUT.csv/-")
             .display_order(1)
             .takes_value(true).required(true))
        .arg(Arg::with_name("output")
             .short("o").long("output")
             .help("Output filename XYZ as grid file (- to write to stdout)")
             .value_name("OUTPUT.xyz[.gz]/-")
             .display_order(2)
             .takes_value(true).required(true))

        .arg(Arg::with_name("res")
             .short("R").long("res")
             .help("Resolution of image (map units per pixel)")
             .value_name("XRES YRES")
             .number_of_values(2)
             .display_order(4)
             .takes_value(true).required(true))

        .arg(Arg::with_name("size")
             .short("s").long("size")
             .help("Size of image (pixels)")
             .value_name("WIDTH HEIGHT")
             .number_of_values(2)
             .display_order(5)
             .takes_value(true))
        
        .arg(Arg::with_name("radius")
             .short("r").long("radius")
             .help("Radius value for heatmap (map units)")
             .value_name("RADIUS")
             .display_order(3)
             .takes_value(true).required(true))


        .arg(Arg::with_name("data_column")
             .short("d").long("data-column")
             .takes_value(true).required(false)
             .value_name("COLUMN_NUMBER")
             .help("Column as the value at that point. Otherwise all points equal.")
             )

        .arg(Arg::with_name("xmin")
             .long("xmin")
             .help("Use this as the xmin of the image rather than use xmin of data")
             .takes_value(true))
        .arg(Arg::with_name("xmax")
             .long("xmax")
             .help("Use this as the xmax of the image rather than use xmax of data")
             .takes_value(true))
        .arg(Arg::with_name("ymin")
             .long("ymin")
             .help("Use this as the ymin of the image rather than use ymin of data")
             .takes_value(true))
        .arg(Arg::with_name("ymax")
             .long("ymax")
             .help("Use this as the ymax of the image rather than use ymax of data")
             .takes_value(true))
        .arg(Arg::with_name("assume_lat_lon")
             .long("assume-lat-lon")
             .help("Input coordinates are treated as lat lon, but all measurements are done with great circle distance in metre. Radius & res is in metres")
             )

        .arg(Arg::with_name("algorithm")
             .long("algorithm")
             .help("Which algorithm to use")
             .takes_value(true).required(false)
             .default_value("quadric")
             .possible_values(&[
                  "uniform", "triangular", "quadric",
                  "triweight", "tricube", "gaussian",
                  "cosine", "logistic", "sigmoid",
                 ])
             )


        .arg(Arg::with_name("compression")
             .short("c").long("compression")
             .takes_value(true).required(false)
             .possible_values(&["none", "auto", "gzip"])
             .default_value("auto")
             .help("Should the output file be compressed?"))

        .get_matches();

    env_logger::init();

    let mut points = vec![];
    let input: Box<dyn Read> = if matches.value_of("input").unwrap() == "-" {
        Box::new(stdin())
    } else {
        Box::new(File::open(matches.value_of("input").unwrap())?)
    };

    let mut csv_reader = csv::ReaderBuilder::new().flexible(true).from_reader(input);

    info!("Reading points from {}", matches.value_of("input").unwrap());
    let mut xmax = None;
    let mut xmin = None;
    let mut ymax = None;
    let mut ymin = None;
    let data_column = matches.value_of("data_column").map(|s| s.parse()).transpose()?;
    for result in csv_reader.records() {
        let record = result?;
        let x: f64 = record.get(0).context("getting x")?.parse()?;
        let y: f64 = record.get(1).context("getting y")?.parse()?;
        let point_value: f64 = data_column.map(|c| record.get(c).context("getting data col")).transpose()?.map(|c| c.parse()).transpose()?.unwrap_or(1.);
        xmax = xmax.map(|xmax| if x > xmax { Some(x) } else { Some(xmax) }).unwrap_or(Some(x));
        xmin = xmin.map(|xmin| if x < xmin { Some(x) } else { Some(xmin) }).unwrap_or(Some(x));
        ymax = ymax.map(|ymax| if y > ymax { Some(y) } else { Some(ymax) }).unwrap_or(Some(y));
        ymin = ymin.map(|ymin| if y < ymin { Some(y) } else { Some(ymin) }).unwrap_or(Some(y));
        points.push(rstar::primitives::PointWithData::new(point_value, [x, y]));
    }
    info!("Read in {} points", points.len());
    let tree = rstar::RTree::bulk_load(points);
    let assume_lat_lon = matches.is_present("assume_lat_lon");

    let radius: f64 = matches.value_of("radius").unwrap().parse()?;
    let radius_sq = radius.powi(2);

    // used for bbox query
    let approx_radius_deg = to_srs_coord(assume_lat_lon, radius);

    let xmin: f64 = match matches.value_of("xmin") { None => xmin.unwrap()-approx_radius_deg, Some(xmin) => xmin.parse()? };
    let xmax: f64 = match matches.value_of("xmax") { None => xmax.unwrap()+approx_radius_deg, Some(xmax) => xmax.parse()? };
    let ymin: f64 = match matches.value_of("ymin") { None => ymin.unwrap()-approx_radius_deg, Some(ymin) => ymin.parse()? };
    let ymax: f64 = match matches.value_of("ymax") { None => ymax.unwrap()+approx_radius_deg, Some(ymax) => ymax.parse()? };


    let xres: f64 = to_srs_coord(assume_lat_lon, matches.values_of("res").unwrap().nth(0).unwrap().parse()?);
    let yres: f64 = to_srs_coord(assume_lat_lon, matches.values_of("res").unwrap().nth(1).unwrap().parse()?);


    let width = ((xmax - xmin)/xres).round() as usize;
    let height = ((ymax - ymin)/yres).round() as usize;

    let output_path = matches.value_of("output").unwrap();
    let output: Box<dyn Write> = match (output_path, matches.value_of("compression").unwrap(), output_path.ends_with(".gz")) {
        ("-", _, _) => {
            Box::new(stdout())
        },
        (_, "gzip", _) | (_, "auto", true) => {
            Box::new(GzEncoder::new(File::create(output_path)?, Compression::default()))
        },
        (_, "none", _) | (_, "auto", false) => {
            Box::new(File::create(output_path)?)
        },
        _ => unreachable!(),
    };
    info!("Saving to {}", output_path);

    let mut output_writer = csv::Writer::from_writer(output);
    output_writer.write_record(&["x", "y", "z"])?;

    let mut value;
    let mut posy; let mut posx;

    let kernel_func = match matches.value_of("algorithm").unwrap() {
        "uniform" => kernel::uniform,
        "triangular" => kernel::triangular,
        "parabolic" => kernel::parabolic,
        "quadric" => kernel::quadric,
        "triweight" => kernel::triweight,
        "tricube" => kernel::tricube,
        "gaussian" => kernel::gaussian,
        "cosine" => kernel::cosine,
        "logistic" => kernel::logistic,
        "sigmoid" => kernel::sigmoid,
        _ => unreachable!(),
    };

    // Only within radius
    let outside_radius_possible = match matches.value_of("algorithm").unwrap() {
        "uniform" => false,
        "triangular" => false,
        "parabolic" => false,
        "quadric" => false,
        "triweight" => false,
        "tricube" => false,
        "gaussian" => true,
        "cosine" => false,
        "logistic" => true,
        "sigmoid" => true,
        _ => unreachable!(),
    };

    let mut x; let mut y; let mut point_value;

    for j in 0..height {
        if j % 100 == 0 {
            info!("{} of {} done", j, height);
        }
        posy = ymin + (j as f64) * yres;
        for i in 0..width {
            posx = xmin + (i as f64) * xres;

            value = 0.;
            for point in tree.locate_in_envelope(
                    &rstar::AABB::from_corners(
                        [posx-approx_radius_deg, posy-approx_radius_deg],
                        [posx+approx_radius_deg, posy+approx_radius_deg]
                    ))
            {

                x = point.position()[0];
                y = point.position()[1];
                point_value = point.data;

                if assume_lat_lon {
                    let dist = haversine_dist(y, x, posy, posx);
                    if outside_radius_possible || dist <= radius {
                        value += point_value*kernel_func(dist/radius);
                    }
                } else {
                    let dist_sq = (x-posx).powi(2) + (y-posy).powi(2);
                    if outside_radius_possible || dist_sq <= radius_sq {
                        value += point_value*kernel_func(dist_sq.sqrt()/radius);
                    }
                }
            }

            output_writer.write_record(&[posx.to_string(), posy.to_string(), value.to_string()])?;
        }
    }
    info!("finished");

    Ok(())
}

fn to_srs_coord(assume_lat_lon: bool, val: f64) -> f64 {
    if assume_lat_lon {
        val / 110_000.
    } else {
        val
    }
}

fn haversine_dist(mut th1: f64, mut ph1: f64, mut th2: f64, ph2: f64) -> f64 {
    ph1 -= ph2;
    ph1 = ph1.to_radians();
    th1 = th1.to_radians();
    th2 = th2.to_radians();
    let dz: f64 = th1.sin() - th2.sin();
    let dx: f64 = ph1.cos() * th1.cos() - th2.cos();
    let dy: f64 = ph1.sin() * th1.cos();
    ((dx * dx + dy * dy + dz * dz).sqrt() / 2.0).asin() * 2.0 * EARTH_RADIUS_M
}


// https://en.wikipedia.org/wiki/Kernel_(statistics)
mod kernel {
    const PI: f64 = std::f64::consts::PI;

    pub(super) fn uniform(_: f64) -> f64 {
        0.5
    }

    pub(super) fn triangular(ratio: f64) -> f64 {
        1. - ratio.abs()
    }

    pub(super) fn parabolic(ratio: f64) -> f64 {
        (3./4.)*(1. - ratio.powi(2))
    }

    pub(super) fn quadric(ratio: f64) -> f64 {
        (15./16.)*(1. - ratio.powi(2)).powi(2)
    }

    pub(super) fn triweight(ratio: f64) -> f64 {
        (35./32.)*(1. - ratio.powi(2)).powi(3)
    }

    pub(super) fn tricube(ratio: f64) -> f64 {
        (70./81.)*(1. - ratio.abs().powi(3)).powi(3)
    }

    pub(super) fn gaussian(ratio: f64) -> f64 {
        1./((2.*PI).sqrt()) * (-0.5 * ratio.powi(2)).exp()
    }

    pub(super) fn cosine(ratio: f64) -> f64 {
        (PI/4.)*(PI*ratio/2.).cos()
    }

    pub(super) fn logistic(ratio: f64) -> f64 {
        1./(ratio.exp() + 2. + (-ratio).exp())
    }

    pub(super) fn sigmoid(ratio: f64) -> f64 {
        (2./PI)*(1./(ratio.exp() + (-ratio).exp()))
    }

}

