<template>
  <div class="hello">
    <h1>{{ msg }}</h1>
    <MyComponent />
  </div>
</template>

<script setup>
import { ref } from 'vue';
import MyComponent from './MyComponent.vue';
import { helper1 } from './helper1.js';

const msg = ref('Hello Vue!' + helper1());
</script>

<style scoped>
h1 {
  color: #42b883;
}
</style>
