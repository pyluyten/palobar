import app from "ags/gtk4/app";
import style from "./styles/main.scss";
import { toggleRunCommand } from "./widgets/RunCommand";

app.start({
  instanceName: "palobar-run-command",
  css: style,
  main() {
    const primary = app.get_monitors()[0];
    toggleRunCommand(primary, () => app.quit());
  },
});
