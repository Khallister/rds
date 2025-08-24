<template>
  <footer class="footer">
    <p>&copy; 2025 Vue DPDM Test App</p>
    <Header />
  </footer>
</template>

<script>
import Header from "./Header.vue";
export default {
  components: {
    Header,
  },
  name: "Footer",
};
</script>

<style scoped>
.footer {
  background-color: #34495e;
  color: white;
  padding: 1rem;
  text-align: center;
}
</style>
