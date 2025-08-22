<template>
  <img 
    class="avatar" 
    :src="src" 
    :alt="alt"
    @error="handleImageError"
  />
</template>

<script>
export default {
  name: 'UserAvatar',
  props: {
    src: String,
    alt: {
      type: String,
      default: 'User avatar'
    }
  },
  methods: {
    handleImageError() {
      console.error('Failed to load avatar image')
    }
  }
}
</script>

<style scoped>
.avatar {
  width: 50px;
  height: 50px;
  border-radius: 50%;
}
</style>
