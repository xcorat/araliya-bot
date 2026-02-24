# araliya-beacon

A floating, always-on-top, transparent GPU-rendered beacon widget for the Araliya bot.

## Overview

`araliya-beacon` is a minimal, borderless, transparent window rendered entirely via
[vello](https://github.com/linebender/vello) (2D GPU vector renderer) on a
[wgpu](https://wgpu.rs/) surface. There is no widget tree — every pixel is drawn
directly into a `vello::Scene`.

It sits on the desktop as a persistent status indicator and launcher for the rest of
the Araliya UI stack.

## Architecture

```
Main thread:    winit event loop + vello/wgpu rendering
Tokio thread:   IPC socket client (sends commands to the araliya daemon)
Channel bridge: EventLoopProxy<UiMessage> for thread-safe UI updates
```

## Visual layout

### Default view

A single hexagon is shown (compact, borderless, transparent window around it).

```
  ╱‾‾╲
 ╱ ·  ╲     ← status dot in centre
 ╲    ╱
  ╲──╱
```

The **status dot** colour reflects the last known daemon connection state:

| Colour | Meaning                      |
|--------|------------------------------|
| Dim    | Idle / no recent reply       |
| Green  | Last IPC call succeeded      |
| Red    | Last IPC call errored        |

### Hover / pinned view

When the pointer **enters** the main hexagon, the window silently expands
(transparent region grows) toward the top-left, and three smaller control hexagons
appear in a sci-fi style:

```
  ⬡ Close        ← exits the program
    ⬡ UI          ← launches / foregrounds araliya-gpui
      ⬡ Settings  ← management UI (placeholder — not yet implemented)
         ╱‾‾╲
        ╱ ·  ╲   ← main hex
        ╲    ╱
         ╲──╱
```

Control hexes are arranged in a short chain toward the **top-left** of the main hex,
reading left-to-right as: Close → UI → Settings.

## Interaction model

| Action                        | Effect                                                             |
|-------------------------------|--------------------------------------------------------------------|
| Hover over main hex           | Show three control hexes (window expands transparently)            |
| Cursor leaves (unpinned)      | Control hexes hide (window shrinks)                                |
| Click main hex (no drag)      | **Toggle pin** — control hexes stay visible when cursor leaves     |
| Click main hex (pinned)       | **Unpin** — control hexes hide when cursor leaves                  |
| Drag main hex                 | Move the window (OS compositor drag); clears pin if set            |
| Drag-release on main hex      | Nothing (no launch, no pin change)                                 |
| Click **Close** hex           | Exit program                                                       |
| Click **UI** hex              | Launch or foreground `araliya-gpui`                                |
| Click **Settings** hex        | Open management UI *(placeholder — not yet implemented)*           |

## Technical notes

- Window: `winit`, borderless, transparent, always-on-top.  
- Renderer: `vello` 2D vector GPU renderer over `wgpu`.  
- IPC: Unix-domain socket client mirroring the wire protocol in `araliya-ctl`; all
  socket calls run on a Tokio runtime and post results back via `EventLoopProxy`.  
- Window resize on hover: the main hex always occupies the same position in logical
  pixel space (bottom-right of the window canvas when expanded, centred when compact),
  so the visual hex never moves when the window grows or shrinks.  
- Hit-testing is done in logical pixels to match winit `CursorMoved` coordinates.

## Related binaries

| Binary          | Role                                     |
|-----------------|------------------------------------------|
| `araliya`       | Main bot daemon                          |
| `araliya-gpui`  | Primary GUI (launched from beacon)       |
| `araliya-ctl`   | CLI management tool                      |
| `araliya-beacon`| This widget                              |

See the crate-level documentation and `docs/architecture/` for the broader system
design.
