extern crate rstar;
extern crate csv;
extern crate clap;
extern crate anyhow;
use clap::{Arg, App};
use std::io::{Write, BufWriter};
use std::fs::*;

use anyhow::{Context, Result};


fn main() -> Result<()> {
    let matches = App::new("My Super Program")
        .arg(Arg::with_name("input")
             .short("i")
             .takes_value(true).required(true))
        .arg(Arg::with_name("output")
             .short("o")
             .takes_value(true).required(true))
        .arg(Arg::with_name("xmin")
             .long("xmin")
             .takes_value(true))
        .arg(Arg::with_name("xmax")
             .long("xmax")
             .takes_value(true))
        .arg(Arg::with_name("ymin")
             .long("ymin")
             .takes_value(true))
        .arg(Arg::with_name("ymax")
             .long("ymax")
             .takes_value(true))
        .arg(Arg::with_name("res")
             .short("R").long("res")
             .number_of_values(2)
             .takes_value(true).required(true))
        .arg(Arg::with_name("size")
             .short("s").long("size")
             .number_of_values(2)
             .takes_value(true))
        .arg(Arg::with_name("radius")
             .short("r").long("radius")
             .takes_value(true).required(true)
             .default_value("10")
             )
        .get_matches();

    let mut points = vec![];
    let mut csv_reader = csv::ReaderBuilder::new().flexible(true).from_path(matches.value_of("input").unwrap())?;
    let mut xmax = None;
    let mut xmin = None;
    let mut ymax = None;
    let mut ymin = None;
    for result in csv_reader.records() {
        let record = result?;
        let x: f64 = record.get(0).context("getting x")?.parse()?;
        let y: f64 = record.get(1).context("getting y")?.parse()?;
        xmax = xmax.map(|xmax| if x > xmax { Some(x) } else { Some(xmax) }).unwrap_or(Some(x));
        xmin = xmin.map(|xmin| if x < xmin { Some(x) } else { Some(xmin) }).unwrap_or(Some(x));
        ymax = ymax.map(|ymax| if y > ymax { Some(y) } else { Some(ymax) }).unwrap_or(Some(y));
        ymin = ymin.map(|ymin| if y < ymin { Some(y) } else { Some(ymin) }).unwrap_or(Some(y));
        points.push([x, y]);
    }
    dbg!(points.len());
    let tree = rstar::RTree::bulk_load(points);

    let radius: f64 = matches.value_of("radius").unwrap().parse()?;
    let radius_sq = radius.powi(2);

    let xmin: f64 = match matches.value_of("xmin") { None => xmin.unwrap()-radius, Some(xmin) => xmin.parse()? };
    let xmax: f64 = match matches.value_of("xmax") { None => xmax.unwrap()+radius, Some(xmax) => xmax.parse()? };
    let ymin: f64 = match matches.value_of("ymin") { None => ymin.unwrap()-radius, Some(ymin) => ymin.parse()? };
    let ymax: f64 = match matches.value_of("ymax") { None => ymax.unwrap()+radius, Some(ymax) => ymax.parse()? };

    let xres: f64 = matches.values_of("res").unwrap().nth(0).unwrap().parse()?;
    let yres: f64 = matches.values_of("res").unwrap().nth(1).unwrap().parse()?;


    let width = ((xmax - xmin)/xres).round() as usize;
    let height = ((ymax - ymin)/yres).round() as usize;
    dbg!(width, height);

    let mut results = vec![0.; width*height];

    let mut new_value;
    for i in 0..width {
        let posx = xmin + (i as f64) * xres;
        for j in 0..height {
            let posy = ymin + (j as f64) * yres;
            new_value = 0.;
            for [x, y] in tree.locate_in_envelope(&rstar::AABB::from_corners([posx-radius, posy-radius], [posx+radius, posy+radius])) {
                let dist_sq = (x-posx).powi(2) + (y-posy).powi(2);
                if dist_sq <= radius_sq {
                    new_value += kde_quatratic(dist_sq.sqrt(), radius);
                }
            }
            results[i*width+j] = new_value;
        }
    }
    
    // print output
    let output_path = matches.value_of("output").unwrap();
    let mut output = BufWriter::new(File::create(output_path)?);
    writeln!(output, "x y z")?;
    for j in 0..height {
        let posy = ymin + (j as f64) * yres;
        for i in 0..width {
            let posx = xmin + (i as f64) * xres;
            writeln!(output, "{} {} {}", posx, posy, results[i*width+j])?;
        }
    }

    let mut output_vrt = BufWriter::new(File::create(format!("{}.vrt", output_path))?);
    write!(output_vrt,
r#"<OGRVRTDataSource>
    <OGRVRTLayer name="{}">
      <SrcDataSource>{}</SrcDataSource> 
      <GeometryType>wkbPoint</GeometryType> 
      <GeometryField encoding="PointFromColumns" x="field_1" y="field_2" z="field_3"/>
    </OGRVRTLayer>
</OGRVRTDataSource>
"#, output_path, output_path)?;
    Ok(())
}

fn kde_quatratic(d: f64, radius: f64) -> f64 {
    let dn = d/radius;
    let p = (15./16.)*(1. - dn.powi(2)).powi(2);

    p
}
