# Third-party notices

This repository contains source code copied from, or derived from, the third-party
projects listed below. Each section names the project, where its code lives in this
repository, and reproduces the project's licence in full. Dependencies that are
fetched from crates.io at build time are not listed here; their licences are in
their own packages.

## sgp4 2.3.0

Source: <https://crates.io/crates/sgp4> (<https://github.com/neuromorphicsystems/sgp4>).
Copied to `crates/sgp4-portable/`, with its math calls routed through the `libm`
crate; `crates/sgp4-portable/README.md` lists every change.

```text
MIT License

Copyright (c) 2020 International Centre for Neuromorphic Systems

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## ERFA 2.0.1

Source: <https://github.com/liberfa/erfa>, tag `v2.0.1`, commit
`9915ba38c9365f8b0738269b8c2ac1fdd5f8dee3`. The IAU 2006/2000A routines that convert
TEME to GCRF are translated into Rust in `crates/ssg-sim/src/orbit/frames/`; each
file there names its ERFA origin, and `nut00a_tables.rs` holds the nutation series
of `nut00a.c`. ERFA is derived, with permission, from the SOFA library; the
translation is neither ERFA nor SOFA software and is not endorsed by either.

```text
Copyright (C) 2013-2021, NumFOCUS Foundation.
All rights reserved.

This library is derived, with permission, from the International
Astronomical Union's "Standards of Fundamental Astronomy" library,
available from http://www.iausofa.org.

The ERFA version is intended to retain identical
functionality to the SOFA library, but made distinct through
different function and file names, as set out in the SOFA license
conditions. The SOFA original has a role as a reference standard
for the IAU and IERS, and consequently redistribution is permitted only
in its unaltered state. The ERFA version is not subject to this
restriction and therefore can be included in distributions which do not
support the concept of "read only" software.

Although the intent is to replicate the SOFA API (other than replacement of
prefix names) and results (with the exception of bugs; any that are
discovered will be fixed), SOFA is not responsible for any errors found
in this version of the library.

If you wish to acknowledge the SOFA heritage, please acknowledge that
you are using a library derived from SOFA, rather than SOFA itself.


TERMS AND CONDITIONS

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1 Redistributions of source code must retain the above copyright
   notice, this list of conditions and the following disclaimer.

2 Redistributions in binary form must reproduce the above copyright
   notice, this list of conditions and the following disclaimer in the
   documentation and/or other materials provided with the distribution.

3 Neither the name of the Standards Of Fundamental Astronomy Board, the
   International Astronomical Union nor the names of its contributors
   may be used to endorse or promote products derived from this software
   without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS
IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A
PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED
TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR
PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF
LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```
