import { createApp } from "vue";
import App from "./App.vue";
import SettingsApp from "./SettingsApp.vue";

// 同一前端按窗口地址分流：#settings 是独立配置窗口，其余是主显示窗。
const Root = window.location.hash.startsWith("#settings") ? SettingsApp : App;

createApp(Root).mount("#app");
