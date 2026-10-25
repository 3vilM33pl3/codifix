// url=https://www.figma.com/design/XLDPSlu5iPKQGqu8SBIEPT?node-id=3-269
// source=desktop/src/ui.tsx
// component=Navigation
import figma from "figma";
const instance = figma.selectedInstance;
const label = instance.getString("Label");
const state = instance.getEnum("State", {
  Default: "default",
  Selected: "selected",
});
export default {
  example: figma.code`<Navigation ${state === "selected" ? "selected" : ""}>${label}</Navigation>`,
  imports: ['import { Navigation } from "./ui"'],
  id: "codifix-navigation",
  metadata: { nestable: true },
};
