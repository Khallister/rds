<template>
  <header class="header">
    <h1>{{ title }}</h1>
    <footer />
  </header>
</template>

<script>
import Footer from "./Footer.vue";
export default {
  name: "Header",
  components: {
    Footer,
  },
  props: {
    title: {
      type: String,
      required: true,
    },
  },
};
</script>

<style scoped>
.header {
  background-color: #42b883;
  color: white;
  padding: 1rem;
}
</style>
