<template>
  <header class="header">
    <h1>{{ title }}</h1>
  </header>
</template>

<script>
export default {
  name: "Header",
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
