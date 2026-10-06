import { createDarkTheme, createLightTheme, type BrandVariants } from "@fluentui/react-components";

/** Our own brand ramp (teal), so the app uses Fluent 2 but doesn't look like a Microsoft product. */
export const brand: BrandVariants = {
  10: "#031312",
  20: "#07211f",
  30: "#0a2f2c",
  40: "#0d3d39",
  50: "#104b46",
  60: "#135954",
  70: "#166861",
  80: "#19776f",
  90: "#1c867d",
  100: "#1f958b",
  110: "#37a399",
  120: "#52b1a8",
  130: "#6dbfb7",
  140: "#8accc6",
  150: "#a8dad5",
  160: "#c6e8e5",
};

export const lightTheme = createLightTheme(brand);
export const darkTheme = {
  ...createDarkTheme(brand),
  // Fluent's default dark brand is a touch dim for our teal; lift the filled-button colour.
  colorBrandBackground: brand[110],
  colorBrandBackgroundHover: brand[120],
  colorBrandBackgroundPressed: brand[100],
  colorNeutralForeground1Static: "#ffffff",
};
