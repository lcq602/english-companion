<script setup lang="ts">
import { computed, ref } from "vue";
import type { HistoryItem } from "../lib/api";
import { speak } from "../lib/api";

const props = defineProps<{ items: HistoryItem[] }>();
const emit = defineEmits<{
  close: [];
  pick: [item: HistoryItem];
  remove: [id: number];
  clear: [];
}>();

const reviewing = ref(false);
const reviewIndex = ref(0);
const revealed = ref(false);

const reviewItem = computed(() => props.items[reviewIndex.value] ?? null);
// 复习时英文在问题面，中文在答案面（若原方向是英译中则反过来）
const reviewQuestion = computed(() => {
  const item = reviewItem.value;
  if (!item) return "";
  return item.target_lang === "en" ? item.translated_text : item.source_text;
});
const reviewAnswer = computed(() => {
  const item = reviewItem.value;
  if (!item) return "";
  return item.target_lang === "en" ? item.source_text : item.translated_text;
});

function startReview() {
  if (!props.items.length) return;
  reviewing.value = true;
  reviewIndex.value = 0;
  revealed.value = false;
}
function next() {
  reviewIndex.value = (reviewIndex.value + 1) % props.items.length;
  revealed.value = false;
}
function prev() {
  reviewIndex.value = (reviewIndex.value - 1 + props.items.length) % props.items.length;
  revealed.value = false;
}
function formatTime(ts: number) {
  return new Date(ts * 1000).toLocaleString("zh-CN", { hour12: false, month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit" });
}
</script>

<template>
  <div class="panel-backdrop" @mousedown.self="emit('close')">
    <section class="panel">
      <header class="panel-heading">
        <div>
          <p class="eyebrow">HISTORY</p>
          <h2>{{ reviewing ? "复习句子" : "翻译历史" }}</h2>
        </div>
        <button class="window-button" type="button" @click="emit('close')" aria-label="关闭">×</button>
      </header>

      <div v-if="reviewing && reviewItem" class="panel-body review-body">
        <p class="review-progress">第 {{ reviewIndex + 1 }} / {{ items.length }} 句</p>
        <div class="review-card" @click="revealed = !revealed">
          <p class="review-question">{{ reviewQuestion }}</p>
          <p v-if="revealed" class="review-answer">{{ reviewAnswer }}</p>
          <p v-else class="review-hint">点击卡片显示译文</p>
        </div>
        <div class="review-actions">
          <button class="secondary-button" type="button" @click="prev">上一句</button>
          <button class="secondary-button" type="button" @click="speak(reviewQuestion)">🔊 朗读</button>
          <button class="secondary-button" type="button" @click="next">下一句</button>
        </div>
        <button class="secondary-button wide" type="button" @click="reviewing = false">返回历史列表</button>
      </div>

      <div v-else class="panel-body">
        <p v-if="!items.length" class="empty-note">还没有翻译记录。在任意输入框打字或复制一段文本，这里就会出现。</p>
        <article v-for="item in items" :key="item.id" class="history-item">
          <button class="history-main" type="button" @click="emit('pick', item)">
            <span class="history-source">{{ item.source_text }}</span>
            <span class="history-result">{{ item.translated_text }}</span>
            <span class="history-meta">{{ formatTime(item.created_at) }} · {{ item.provider }}</span>
          </button>
          <button class="window-button" type="button" @click="emit('remove', item.id)" aria-label="删除这条">×</button>
        </article>
      </div>

      <footer v-if="!reviewing" class="panel-actions">
        <button class="secondary-button" type="button" :disabled="!items.length" @click="startReview">开始复习</button>
        <button class="secondary-button danger" type="button" :disabled="!items.length" @click="emit('clear')">清空历史</button>
        <button class="primary-button" type="button" @click="emit('close')">关闭</button>
      </footer>
    </section>
  </div>
</template>
