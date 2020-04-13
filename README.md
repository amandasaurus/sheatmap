# sheatmap

Generate heatmap image data from input points. Usable easily with gdal to make
heatmap geotiffs from points.

gdal can convert a point dataset (e.g. shapefile, geojson file, etc) into a CSV file, with the X & Y as separate columns. Likewise a CSV file can be converted to a raster file (e.g. GeoTIFF). `gdal_grid` converts several vector files to raster files, but it doesn't yet have a 'heatmap' function, so I built this to do that

## Installation

    cargo install sheatmap

## Usage


A Shapefile can be converted to a CSV file with this:

    ogr2ogr -f CSV -lco GEOMETRY=AS_XY output.csv input.shp

Create the heatmap raster-as-CSV:

    sheatmap -i $< -o $@ --res $(res) $(res) --radius $(radius)

Convert the raster-as-CSV to GeoTIFF:

    gdal_translate -a_srs epsg:$(srs) /vsigzip/$< $@


## Copyright

© 2020 GNU Affero GPL v3 (or later), see [LICENCE](LICENCE).
