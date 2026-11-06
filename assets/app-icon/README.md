# PhotoCAT app icon

The letters **PC** on a dark navy rounded square, the P light blue and the C orange. It takes the general
feel of a dark tile with bright letters, and is not a copy of any other product's logo.

| Colour | Hex | Used for |
|---|---|---|
| Navy | `#0B2138` | Tile |
| Light blue | `#63C4FF` | P |
| Orange | `#FFA94D` | C |

`photocraft.svg` is the master; the file names stay as they are because the build and packaging
scripts refer to them. `photocraft-small.svg` is a copy of it. Regenerate every derived file with
`packaging/icons.sh` (needs `resvg`).
