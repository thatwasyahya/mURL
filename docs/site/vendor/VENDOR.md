# Vendored front-end dependencies

Pinned copies, served from this origin. The site makes no request to a
third-party host at runtime: no CDN script, no font service. For a tool whose
whole argument is that a click should not run code from a stranger, loading
its own pages from strangers would be a poor look, and self-hosting the fonts
also keeps EU visitors out of the Google Fonts problem.

To upgrade: download the new version to the same path, re-run the hash
command below, and update the table. Nothing here is transformed.

| File | Source | Version | License |
|---|---|---|---|
| highlight.min.js | cdnjs highlight.js | 11.11.1 | BSD-3-Clause |
| motion.js | npm motion (UMD) | 12.23.12 | MIT |
| fonts/ibm-plex-*.woff2 | npm @fontsource/ibm-plex-sans, ibm-plex-mono (latin) | 5.2.6 | OFL-1.1 |
| fonts/instrument-serif-*.woff2 | npm @fontsource/instrument-serif (latin) | 5.2.6 | OFL-1.1 |
| icons.svg | built from npm lucide-static icons | 0.544.0 | ISC |

## SHA-256

```
sha256sum highlight.min.js motion.js fonts/*.woff2
```

```
c4a399dd6f488bc97a3546e3476747b3e714c99c57b9473154c6fb8d259b9381  highlight.min.js
cbaccc5c5809cdaa2777ded956e475a404f0596048cb9645c8c80da85c6e8174  motion.js
3c5a451f9ec27a354b0c2bcca636c6ec17a651281aabf29f8427e210a1d31e85  fonts/ibm-plex-mono-400.woff2
756026ff72eb76fd971ac4b7504cec55eef62109d2684c2cad8da32170b80b37  fonts/ibm-plex-mono-500.woff2
c4d3deb734a27e6d0dc7a6b464779f70ba1c272e26287860a14e35e85acb5b76  fonts/ibm-plex-mono-600.woff2
6de912e531b6c98084f1b2d5e5a91bad77be4e68bc4e396e43c46fc435e5f3d9  fonts/ibm-plex-sans-400-italic.woff2
3b646991d30055a93a4ecc499713d4347953a74a947ecab435ab72070cbdab0e  fonts/ibm-plex-sans-400.woff2
0717336fb31fcdcde4b8deb3675bb4a0f7f6d484864afcd6751ac29975962203  fonts/ibm-plex-sans-500.woff2
8960851d691c054ed38e259bdcf1a6190d157b4203ed5bb32c632a863fb8ec2f  fonts/ibm-plex-sans-600.woff2
42e7b0c143c19df9d99fd896e76b48f846edf0902d200bc29796b34d12c33aa7  fonts/ibm-plex-sans-700.woff2
1d6e1bd7bc12e2920ed13edb467b8a5ec4a344e6fb78eb9e302ad9ab00981b9c  fonts/instrument-serif-400-italic.woff2
7796998dac1ab02b98c32b6e2babbd56255ff3b4e9681d9c7c608530d9033eb6  fonts/instrument-serif-400.woff2
```
