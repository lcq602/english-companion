<script setup lang="ts">
import type { WordDetail } from "../lib/api";
import { speak } from "../lib/api";

defineProps<{ detail: WordDetail; loading: boolean; error: string }>();
const emit = defineEmits<{ close: []; favorite: [detail: WordDetail] }>();
</script>

<template>
  <section class="word-card">
    <button class="window-button word-close" type="button" @click="emit('close')" aria-label="关闭单词卡">×</button>
    <p v-if="loading" class="word-status">查词中…</p>
    <p v-else-if="error" class="word-status error">{{ error }}</p>
    <template v-else>
      <div class="word-head">
        <span class="word-title">{{ detail.word }}</span>
        <span v-if="detail.phonetic" class="word-phonetic">{{ detail.phonetic }}</span>
        <button class="mini-button" type="button" @click="speak(detail.word, detail.audio_url)">🔊 发音</button>
        <button class="mini-button" type="button" @click="emit('favorite', detail)">★ 收藏</button>
      </div>
      <p v-if="detail.meaning_zh" class="word-meaning-zh">{{ detail.meaning_zh }}</p>
      <ul v-if="detail.definitions.length" class="word-defs">
        <li v-for="def in detail.definitions" :key="def">{{ def }}</li>
      </ul>
    </template>
  </section>
</template>
