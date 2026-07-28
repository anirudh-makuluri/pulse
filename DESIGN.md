---
name: Midnight Forest
colors:
  surface: '#161311'
  surface-dim: '#161311'
  surface-bright: '#3d3836'
  surface-container-lowest: '#110d0c'
  surface-container-low: '#1f1b19'
  surface-container: '#231f1d'
  surface-container-high: '#2e2927'
  surface-container-highest: '#393431'
  on-surface: '#eae1dd'
  on-surface-variant: '#c2c8c0'
  inverse-surface: '#eae1dd'
  inverse-on-surface: '#342f2d'
  outline: '#8c928b'
  outline-variant: '#424843'
  surface-tint: '#adcfb4'
  primary: '#adcfb4'
  on-primary: '#183624'
  primary-container: '#2d4b37'
  on-primary-container: '#99baa1'
  inverse-primary: '#466550'
  secondary: '#b9ccb0'
  on-secondary: '#253421'
  secondary-container: '#3d4d38'
  on-secondary-container: '#abbea2'
  tertiary: '#f0bd8b'
  on-tertiary: '#482904'
  tertiary-container: '#603d16'
  on-tertiary-container: '#daa978'
  error: '#ffb4ab'
  on-error: '#690005'
  error-container: '#93000a'
  on-error-container: '#ffdad6'
  primary-fixed: '#c8ebd0'
  primary-fixed-dim: '#adcfb4'
  on-primary-fixed: '#022110'
  on-primary-fixed-variant: '#2f4d39'
  secondary-fixed: '#d5e8cb'
  secondary-fixed-dim: '#b9ccb0'
  on-secondary-fixed: '#101f0d'
  on-secondary-fixed-variant: '#3b4b36'
  tertiary-fixed: '#ffdcbd'
  tertiary-fixed-dim: '#f0bd8b'
  on-tertiary-fixed: '#2c1600'
  on-tertiary-fixed-variant: '#623f18'
  background: '#161311'
  on-background: '#eae1dd'
  surface-variant: '#393431'
typography:
  headline-xl:
    fontFamily: Inter
    fontSize: 40px
    fontWeight: '700'
    lineHeight: 48px
    letterSpacing: -0.02em
  headline-xl-mobile:
    fontFamily: Inter
    fontSize: 32px
    fontWeight: '700'
    lineHeight: 40px
    letterSpacing: -0.02em
  headline-lg:
    fontFamily: Inter
    fontSize: 28px
    fontWeight: '600'
    lineHeight: 36px
    letterSpacing: -0.01em
  headline-md:
    fontFamily: Inter
    fontSize: 20px
    fontWeight: '600'
    lineHeight: 28px
    letterSpacing: 0em
  body-lg:
    fontFamily: Inter
    fontSize: 18px
    fontWeight: '400'
    lineHeight: 28px
    letterSpacing: 0em
  body-md:
    fontFamily: Inter
    fontSize: 16px
    fontWeight: '400'
    lineHeight: 24px
    letterSpacing: 0em
  label-md:
    fontFamily: Inter
    fontSize: 14px
    fontWeight: '500'
    lineHeight: 20px
    letterSpacing: 0.01em
  label-sm:
    fontFamily: Inter
    fontSize: 12px
    fontWeight: '600'
    lineHeight: 16px
    letterSpacing: 0.05em
rounded:
  sm: 0.25rem
  DEFAULT: 0.5rem
  md: 0.75rem
  lg: 1rem
  xl: 1.5rem
  full: 9999px
spacing:
  base: 4px
  gutter: 24px
  margin-mobile: 16px
  margin-desktop: 40px
  stack-sm: 8px
  stack-md: 16px
  stack-lg: 32px
---

## Brand & Style
The design system is built upon the "Midnight Forest" narrative—a sophisticated blend of organic warmth and technical precision. It targets professional environments that prioritize reliability and "local-first" data integrity, evoking a sense of grounded stability and quiet focus.

The visual style is a hybrid of **Minimalism** and **Tactile Depth**. It uses a foundation of deep, earthen shadows and rich, mossy accents to create a high-end, immersive dark mode experience. The atmosphere is calm and professional, avoiding the sterile "blue-black" of typical SaaS platforms in favor of a natural, organic palette that feels more human and sustainable.

## Colors
The palette is rooted in a "near-black" brown foundation, providing a warmer, more legible base than pure black. 

- **Primary Forest:** A rich, saturated green used for primary actions and brand presence.
- **Muted Moss (Secondary):** A desaturated, atmospheric green for secondary UI elements and inactive states.
- **Earth Tones (Tertiary/Neutral):** Muted browns and warm clay tones are used for container backgrounds and subtle borders to maintain the organic feel.
- **Semantic Colors:** Success, warning, and error states should be desaturated to fit the earthy environment (e.g., a "clay red" instead of bright "apple red").

## Typography
This design system utilizes **Inter** exclusively to provide a technical, high-legibility contrast to the organic color palette. 

The type hierarchy is structured for density and clarity. Headlines utilize tighter letter spacing and heavier weights to command attention, while body text remains open and legible. For small labels and metadata, a slightly increased letter spacing and uppercase styling is recommended to ensure "local-first" technical details are easily scannable against dark backgrounds.

## Layout & Spacing
The layout follows a **Fluid Grid** model with a focus on internal density. It uses an 8px spatial rhythm for all padding and margins.

- **Desktop:** 12-column grid with 24px gutters. Wide margins (40px) allow the dark background to frame the content, enhancing the "forest" atmosphere.
- **Tablet:** 8-column grid with 20px gutters. 
- **Mobile:** 4-column grid with 16px gutters and 16px margins.
- **Rhythm:** Vertical spacing between sections should be generous to avoid visual clutter, while internal component spacing remains tight and technical.

## Elevation & Depth
Depth is created through **Tonal Layers** and subtle **Ambient Shadows**. Because the background is a very dark brown, we avoid pure black shadows.

1. **Base Layer:** The darkest brown-black (#12100E).
2. **Container Layer:** A slightly lighter brown (#1C1917) with a subtle 1px border in a muted moss or earth tone to define boundaries.
3. **Elevated Elements:** Use a "Forest Glow" shadow—a low-opacity dark green or deep brown shadow that feels like a natural shadow in a forest clearing rather than a digital drop shadow.
4. **Interaction:** Hover states should lift elements slightly using a subtle increase in border-luminance rather than massive shadow changes.

## Shapes
A consistent **8px (0.5rem)** corner radius is applied to all primary containers, buttons, and input fields. This "Rounded" approach softens the technical nature of Inter and the dark theme, making the UI feel more organic and approachable. 

- Large cards and modals use `rounded-xl` (1.5rem).
- Small tags or utility buttons use `rounded-lg` (1rem) for a distinct silhouette.

## Components

- **Buttons:** Primary buttons use the rich forest green with white or high-contrast cream text. Secondary buttons are "Ghost" style with a muted moss border.
- **Cards:** Cards should have a subtle background-color difference from the main canvas (using the Surface tone) and a 1px border in a dark earth shade (#2A2623).
- **Input Fields:** Use a darker "Well" effect—backgrounds that are slightly darker than the card surface, with an 8px radius. The focus state uses a forest green ring.
- **Chips/Tags:** Small, pill-shaped elements with low-opacity moss green backgrounds and technical Inter labels.
- **Lists:** Use subtle horizontal dividers in muted brown. Ensure ample vertical padding (12px-16px) to maintain the "calm" atmosphere.
- **Selection Controls:** Checkboxes and radio buttons should feel tactile; when selected, they use the primary forest green with a slight inner glow.