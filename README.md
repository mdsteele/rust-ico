# rust-ico

A pure Rust library for encoding/decoding
[ICO image files](https://en.wikipedia.org/wiki/ICO_%28file_format%29).

Documentation: https://docs.rs/ico/

## Overview

An ICO file (.ico) stores a collection of small images of different sizes and
color depths (up to 256x256 pixels each).  Individial images within the file
can be encoded in either BMP or PNG format.  ICO files are typically used for
website favicons and for Windows application icons.

CUR files (.cur), which store Windows cursor images, use the same file format
as ICO files, except that each image also comes with (x, y) *hotspot*
coordinates that determines where on the image the user is pointing.  This
libary supports both file types.

## License

rust-ico is made available under the
[MIT License](http://spdx.org/licenses/MIT.html).
