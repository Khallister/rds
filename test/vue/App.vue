<template>
  <div class="app">
    <Header :title="appTitle" />
    <UserProfile :user="currentUser" />
    <Footer />
  </div>
</template>

<script>
import Header from './components/Header.vue'
import UserProfile from './components/UserProfile.vue'
import Footer from './components/Footer.vue'
import { getUser } from './utils/api.js'
import { validateUser } from './utils/validation.js'

export default {
  name: 'App',
  components: {
    Header,
    UserProfile,
    Footer,
  },
  data() {
    return {
      appTitle: 'Vue DPDM Test',
      currentUser: null,
    }
  },
  async created() {
    this.currentUser = await getUser(1)
    if (!validateUser(this.currentUser)) {
      console.error('Invalid user data')
    }
  }
}
</script>

<style scoped>
.app {
  font-family: Avenir, Helvetica, Arial, sans-serif;
  text-align: center;
  color: #2c3e50;
  margin-top: 60px;
}
</style>
