<script setup lang="ts">
import { ref } from "vue";
import type { AppSettings, ProviderId } from "../lib/settings";
import { PROVIDER_NOTES } from "../lib/settings";

const props = defineProps<{ settings: AppSettings }>();
const emit = defineEmits<{ save: [settings: AppSettings]; close: [] }>();
const draft = ref<AppSettings>({ ...props.settings });

const providers: { id: ProviderId; label: string }[] = [
  { id: "google", label: "Google Translate（默认）" },
  { id: "microsoft", label: "Microsoft Translator（官方，需 Key）" },
  { id: "mymemory", label: "MyMemory（免费）" },
  { id: "libre", label: "LibreTranslate（可自建）" },
];

function save() {
  emit("save", { ...draft.value, opacity: Number(draft.value.opacity) });
}
</script>

<template>
  <div class="panel-backdrop" @mousedown.self="emit('close')">
    <section class="panel">
      <header class="panel-heading">
        <div>
          <p class="eyebrow">SETTINGS</p>
          <h2>设置</h2>
        </div>
        <button class="window-button" type="button" @click="emit('close')" aria-label="关闭设置">×</button>
      </header>

      <div class="panel-body">
        <label>
          翻译服务
          <select v-model="draft.provider">
            <option v-for="p in providers" :key="p.id" :value="p.id">{{ p.label }}</option>
          </select>
        </label>
        <p class="note">{{ PROVIDER_NOTES[draft.provider] }}</p>

        <label>
          备用服务（主服务失败时自动切换）
          <select v-model="draft.fallbackProvider">
            <option value="">不启用</option>
            <option v-for="p in providers.filter((p) => p.id !== draft.provider)" :key="p.id" :value="p.id">{{ p.label }}</option>
          </select>
        </label>

        <template v-if="draft.provider === 'microsoft' || draft.fallbackProvider === 'microsoft'">
          <label>Microsoft API Key<input v-model="draft.microsoftKey" type="password" placeholder="Azure Translator Key" autocomplete="off" /></label>
          <label>Microsoft 区域（多区域资源才需要）<input v-model="draft.microsoftRegion" placeholder="例如 eastasia，可留空" autocomplete="off" /></label>
        </template>

        <template v-if="draft.provider === 'libre' || draft.fallbackProvider === 'libre'">
          <label>LibreTranslate 服务地址<input v-model="draft.libreUrl" placeholder="https://libretranslate.com 或自建地址" autocomplete="off" /></label>
          <label>LibreTranslate API Key（自建免 Key 可留空）<input v-model="draft.libreKey" type="password" autocomplete="off" /></label>
        </template>

        <template v-if="draft.provider === 'mymemory' || draft.fallbackProvider === 'mymemory'">
          <label>MyMemory 邮箱（可选，填了提升免费额度）<input v-model="draft.mymemoryEmail" placeholder="you@example.com" autocomplete="off" /></label>
        </template>

        <label>
          请求超时（秒）：{{ draft.timeoutSecs }}
          <input v-model.number="draft.timeoutSecs" type="range" min="3" max="30" step="1" />
        </label>

        <h3>悬浮窗</h3>
        <label>
          透明度：{{ Math.round(draft.opacity * 100) }}%
          <input v-model.number="draft.opacity" type="range" min="0.35" max="1" step="0.01" />
        </label>
        <label class="switch-row"><input v-model="draft.alwaysOnTop" type="checkbox" /><span>始终置顶</span></label>
        <label class="switch-row"><input v-model="draft.bilingual" type="checkbox" /><span>中英对照（关闭为纯英文模式）</span></label>

        <h3>监听与安全</h3>
        <label class="switch-row"><input v-model="draft.uiaEnabled" type="checkbox" /><span>自动监听当前输入框（Windows UI Automation，密码框自动跳过）</span></label>
        <label class="switch-row"><input v-model="draft.clipboardEnabled" type="checkbox" /><span>监听剪贴板（复制文本自动翻译）</span></label>
        <label>
          应用黑名单（每行一个进程名片段，命中的不监听）
          <textarea v-model="draft.blacklist" rows="4" spellcheck="false" />
        </label>
        <p class="note">隐私：不记录原始键盘输入日志；只保存你实际翻译过的内容（本地 SQLite 缓存与历史）。监听可随时用 Ctrl+Shift+P 一键暂停。</p>
      </div>

      <footer class="panel-actions">
        <button class="secondary-button" type="button" @click="emit('close')">取消</button>
        <button class="primary-button" type="button" @click="save">保存设置</button>
      </footer>
    </section>
  </div>
</template>
