# Effects Catalog

Long-form brainstorm of demoscene / generative effects, grouped by aesthetic.

**Status legend**
- ✅ Shipped (scene name in parens links to file in `scenes/`)
- 📋 Queued (see `ROADMAP.md::Execution Order`)
- ☐ Not yet built

Cross-referenced against `scenes/` on 2026-05-10.

---

## Gritty / digital glitch

- ✅ CRT signal collapse (`crt_collapse`)
- ✅ Phosphor CRT — softer version of CRT collapse (`phosphor_crt`)
- ✅ VHS tracking (`vhs_tracking`)
- ✅ Datamosh blocks (`datamosh`)
- ✅ Hex/binary rain (`hex_rain`)
- ✅ ASCII rain — Matrix-style anime variant (`ascii_rain`)
- ✅ Generic glitch effect (`glitch`)
- ✅ TV static / snow (`static`)
- ✅ Bayer 1-bit dither (`bayer`)
- ☐ Pixel-sort by brightness threshold
- ☐ RGB channel slip (animated separation)
- ☐ Macroblock breakup (codec artifact, simpler cousin of datamosh)
- ☐ Modem connecting handshake bands
- ☐ Scanline tear
- ☐ Signal sync-loss roll
- ☐ 4-color CGA posterization
- ☐ Bit-rot decay (slowly corrupts `u_prev` — feedback decay aesthetic)
- ☐ Dead-pixel grid (random sparse hot pixels)

## Classic demoscene

- ✅ Plasma (`plasma`)
- ✅ Tunnel (`tunnel`, `tunnel_mirrors`)
- ✅ Starfield (`starfield`)
- ✅ Metaballs (`metaballs`)
- ✅ 3D wireframe object spin (`cube_wireframe`)
- ☐ Rotozoom (rotated zoomed bitmap)
- ☐ Fire (Doom heat-bleed cellular automaton)
- ☐ Sprite scroller / greetz banner
- ☐ Lens flare
- ☐ DVD bouncing-logo
- ☐ 2D Mode-7 floor
- ✅ Voxel terrain (`voxel_terrain`)
- ☐ Old-school 3D dot tunnel
- ✅ donut.c spinning ASCII donut (`donut`)

## 3D fractals & raymarching

- ✅ Mandelbrot (infinite zoom) (`mandelbrot`)
- ✅ Mandelbulb (`mandelbulb`)
- ✅ Mandelbox (`mandelbox`)
- ✅ Juliabulb (`juliabulb`)
- ✅ Menger Sponge (`menger_sponge`)
- ✅ Sierpinski 3D (`sierpinski_3d`)
- ✅ Apollonian (`apollonian`)
- ✅ Pseudo-Kleinian (`kleinian`)

## Cyberpunk / terminal

- ✅ BIOS POST scroll (`bios_post`)
- ☐ NORAD missile-tracking radar (faux threat overlay)
- ☐ ASCII world map with packet flow
- ☐ Faux IRC scrollback
- ☐ Terminal cursor blink-storm
- ☐ Faux disassembly scroll (rolling x86 mnemonics)
- ☐ Connection stack-trace cascade
- ☐ Schematic/blueprint scan reveal

## Synthwave / vector / geometric

- ✅ Synthwave wireframe sun + gridfloor (`synthwave_grid`)
- ✅ Kaleidoscope (`kaleidoscope`)
- ✅ Lissajous figures (`lissajous`)
- ✅ Voronoi cells (`voronoi`)
- ✅ Hex grid wave (`hex_grid`)
- ✅ Truchet tiles (`truchet`)
- ✅ Pulse grid (`pulse_grid`)
- ☐ Rotating mandala
- ☐ Sierpinski 2D recursive triangles
- ☐ Lorenz attractor 3D
- ☐ Polar rose curves
- ☐ Spirograph

## Organic / nature

- ✅ Curl-noise fluid flow (`curl_noise`)
- ✅ Underwater caustics (`caustics`)
- ✅ Reaction-diffusion / Gray-Scott (`reaction_diffusion`)
- ✅ Conway's Game of Life (`conway`)
- ☐ Audio-reactive lightning bolts (fork tree on bass hit)
- ✅ Smoke / ink dispersion (`smoke`)
- ☐ Cloud noise drift
- ☐ Sandpile cellular automaton

## Pop / anime

- ✅ Speed lines (`speed_lines`)
- ✅ Ben-Day halftone dots (`halftone`)
- ✅ Sailor-Moon transformation rings (`transform_rings`)
- ✅ Sparkle burst (`sparkle`)

## Architectural

- ☐ Penrose tiling
- ☐ Escher impossible tile loop
- ☐ Folding cube illusion

## Typography

- ☐ Big rotating logo
- ☐ Scrolling marquee
- ☐ Text explosion into particles
- ☐ Text typed-out terminal effect

## Optics

- ☐ Bloom over a moving spot
- ☐ Caustic ray bender (separate from the underwater caustics)
- ☐ Prism rainbow split
- ☐ Anamorphic lens streak
- ☐ Solarization

## Game references

- ✅ 3D Pipes screensaver (`pipes_3d`)
- ✅ 3D Maze screensaver (`maze_3d`)
- ✅ Pong field self-playing (`pong`)
- ☐ Tetris piece rain
- ☐ Pac-Man dot trail across screen
- ☐ 80s Donkey Kong barrels
- ☐ Galaga starfield

## Physics / simulation

- ☐ N-body gravity dance
- ☐ Spring lattice wave
- ✅ Flocking boids (`boids`)
- ✅ Pond ripples (`pond`)
- ☐ Vortex shedding around obstacle

## Experimental / time-domain

- ✅ Slit-scan time smear (`slit_scan`)
- ✅ Feedback delay (`echo`, `mirror_delay`)
- ☐ Color-space rotation
- ☐ Polar warp on a bitmap
- ☐ Self-affine recursion (audio modulates scale → infinite zoom into itself)

## Audio scopes / dashboards

- ✅ Spectrogram waterfall (`spectrogram_waterfall`)
- ✅ Spectrum bars (`spectrum_bars`)
- ✅ Waveform line / oscilloscope (`waveform_line`)
- ✅ Strobe (`strobe`)
- ✅ Shockwave (`shockwave`)
- ✅ Vinyl record (`vinyl`)
- ✅ Audio vectorscope (`vectorscope`)
- ☐ Stereo phase display
- ☐ Faux dashboard cluster (speedo + tach reacting to audio)

## Misc / debug

- ✅ Solid color (debug) (`solid`)
- ✅ SMPTE bars (`__safe__`, baked-in fallback)

---

## Where to look next

Strong picks from the remaining ☐ items, by category:
- **Fire (Doom heat-bleed CA)** — fills the heat/flame gap. Classic.
- **Tetris piece rain** — most recognizable game ref still missing.
- **NORAD radar / packet flow** — best of the cyberpunk-terminal gaps.
- **Sandpile CA** or **Lenia** — fresh CA aesthetic distinct from reaction-diffusion.
- **Faux dashboard cluster** — audio-reactive speedo+tach reads great on composite.
- **Lorenz attractor 3D** — the strongest "math classroom" geometric not yet built.
