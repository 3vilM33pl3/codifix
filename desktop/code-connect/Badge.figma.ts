// url=https://www.figma.com/design/XLDPSlu5iPKQGqu8SBIEPT?node-id=3-278
// source=desktop/src/ui.tsx
// component=Badge
import figma from "figma";
const instance = figma.selectedInstance;
const label = instance.getString("Label");
const tone = instance.getEnum("Tone", {
  Neutral: "",
  Positive: "positive",
  Warning: "warning",
  Danger: "danger",
});
export default {
  example: figma.code`<Badge tone="${tone}">${label}</Badge>`,
  imports: ['import { Badge } from "./ui"'],
  id: "codifix-badge",
  metadata: { nestable: true },
};
