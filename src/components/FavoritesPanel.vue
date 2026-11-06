<script setup lang="ts">
import type { FavoriteWord } from "../lib/api";
import { speak } from "../lib/api";

defineProps<{ items: FavoriteWord[] }>();
const emit = defineEmits<{ close: []; remove: [word: string] }>();
</script>

<template>
  <div class="panel-backdrop" @mousedown.self="emit('close')">
    <section class="panel">
      <header class="panel-heading">
        <div>
          <p class="eyebrow">WORDS</p>
          <h2>收藏单词</h2>
        </div>
        <button class="window-button" type="button" @click="emit('close')" aria-label="关闭">×</button>
      </header>
      <div class="panel-body">
        <p v-if="!items.length" class="empty-note">还没有收藏。点击译文中的英文单词，查到释义后点 ★ 就能收藏到这里复习。</p>
        <article v-for="item in items" :key="item.id" class="word-item">
          <div class="word-main">
            <span class="word-text">{{ item.word }}</span>
            <span v-if="item.phonetic" class="word-phonetic">{{ item.phonetic }}</span>
            <span class="word-meaning">{{ item.meaning }}</span>
          </div>
          <button class="window-button" type="button" @click="speak(item.word)" aria-label="发音">🔊</button>
          <button class="window-button" type="button" @click="emit('remove', item.word)" aria-label="取消收藏">×</button>
        </article>
      </div>
      <footer class="panel-actions">
        <button class="primary-button" type="button" @click="emit('close')">关闭</button>
      </footer>
    </section>
  </div>
</template>
