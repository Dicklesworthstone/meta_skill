# Python Rich Export Fixtures (HTML/SVG)

These fixtures capture **Python Rich** export output for manual comparison
against `rich_rust`'s HTML/SVG exporters.

Generated on: 2026-01-25  
Python Rich: 13.9.4  
Console config: `width=40`, `color_system=truecolor`, `force_terminal=True`, `record=True`

Input sequence:

```
console.print("Plain")
console.print("[bold red]Error[/]")
console.print("[link=https://example.com]Link[/]")
```

Notes:
- Python Rich exporters clear the record buffer by default; these fixtures use `clear=False` so
  HTML and SVG are generated from the same recorded output.
- Python Rich SVG output uses a `terminal-<id>` / `<unique_id>` prefix in CSS class names and ids.
- HTML output below is inline-styles mode (`export_html(clear=False, inline_styles=True)`).

## HTML (inline styles)

```html
<!DOCTYPE html>
<html>
<head>
<meta charset="UTF-8">
<style>

body {
    color: #000000;
    background-color: #ffffff;
}
</style>
</head>
<body>
    <pre style="font-family:Menlo,'DejaVu Sans Mono',consolas,'Courier New',monospace"><code style="font-family:inherit">Plain
<span style="color: #800000; text-decoration-color: #800000; font-weight: bold">Error</span>
<a href="https://example.com">Link</a>
</code></pre>
</body>
</html>
```

## SVG

```svg
<svg class="rich-terminal" viewBox="0 0 506 123.19999999999999" xmlns="http://www.w3.org/2000/svg">
    <!-- Generated with Rich https://www.textualize.io -->
    <style>

    @font-face {
        font-family: "Fira Code";
        src: local("FiraCode-Regular"),
                url("https://cdnjs.cloudflare.com/ajax/libs/firacode/6.2.0/woff2/FiraCode-Regular.woff2") format("woff2"),
                url("https://cdnjs.cloudflare.com/ajax/libs/firacode/6.2.0/woff/FiraCode-Regular.woff") format("woff");
        font-style: normal;
        font-weight: 400;
    }
    @font-face {
        font-family: "Fira Code";
        src: local("FiraCode-Bold"),
                url("https://cdnjs.cloudflare.com/ajax/libs/firacode/6.2.0/woff2/FiraCode-Bold.woff2") format("woff2"),
                url("https://cdnjs.cloudflare.com/ajax/libs/firacode/6.2.0/woff/FiraCode-Bold.woff") format("woff");
        font-style: bold;
        font-weight: 700;
    }

    .py-fixture-matrix {
        font-family: Fira Code, monospace;
        font-size: 20px;
        line-height: 24.4px;
        font-variant-east-asian: full-width;
    }

    .py-fixture-title {
        font-size: 18px;
        font-weight: bold;
        font-family: arial;
    }

    .py-fixture-r1 { fill: #c5c8c6 }
.py-fixture-r2 { fill: #cc555a;font-weight: bold }
    </style>

    <defs>
    <clipPath id="py-fixture-clip-terminal">
      <rect x="0" y="0" width="487.0" height="72.19999999999999" />
    </clipPath>
    <clipPath id="py-fixture-line-0">
    <rect x="0" y="1.5" width="488" height="24.65"/>
            </clipPath>
<clipPath id="py-fixture-line-1">
    <rect x="0" y="25.9" width="488" height="24.65"/>
            </clipPath>
    </defs>

    <rect fill="#292929" stroke="rgba(255,255,255,0.35)" stroke-width="1" x="1" y="1" width="504" height="121.2" rx="8"/><text class="py-fixture-title" fill="#c5c8c6" text-anchor="middle" x="252" y="27">Rich</text>
            <g transform="translate(26,22)">
            <circle cx="0" cy="0" r="7" fill="#ff5f57"/>
            <circle cx="22" cy="0" r="7" fill="#febc2e"/>
            <circle cx="44" cy="0" r="7" fill="#28c840"/>
            </g>
        
    <g transform="translate(9, 41)" clip-path="url(#py-fixture-clip-terminal)">
    
    <g class="py-fixture-matrix">
    <text class="py-fixture-r1" x="0" y="20" textLength="61" clip-path="url(#py-fixture-line-0)">Plain</text><text class="py-fixture-r1" x="488" y="20" textLength="12.2" clip-path="url(#py-fixture-line-0)">
</text><text class="py-fixture-r2" x="0" y="44.4" textLength="61" clip-path="url(#py-fixture-line-1)">Error</text><text class="py-fixture-r1" x="488" y="44.4" textLength="12.2" clip-path="url(#py-fixture-line-1)">
</text><text class="py-fixture-r1" x="0" y="68.8" textLength="48.8" clip-path="url(#py-fixture-line-2)">Link</text><text class="py-fixture-r1" x="488" y="68.8" textLength="12.2" clip-path="url(#py-fixture-line-2)">
</text>
    </g>
    </g>
</svg>
```
