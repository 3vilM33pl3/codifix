// url=https://www.figma.com/design/XLDPSlu5iPKQGqu8SBIEPT?node-id=3-264
// source=desktop/src/ui.tsx
// component=Button
import figma from "figma";
const instance = figma.selectedInstance;
const label = instance.getString("Label");
const style = instance.getEnum("Style", {
  Primary: "primary",
  Secondary: "secondary",
  Ghost: "ghost",
  Disabled: "disabled",
});
export default {
  example: figma.code`<Button variant="${style === "disabled" ? "secondary" : style}" ${style === "disabled" ? "disabled" : ""}>${label}</Button>`,
  imports: ['import { Button } from "./ui"'],
  id: "codifix-button",
  metadata: { nestable: true },
};
