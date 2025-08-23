<template>
  <div class="user-profile">
    <h2>{{ user.name }}</h2>
    <p>{{ user.email }}</p>
    <UserAvatar :src="user.avatar" />
  </div>
</template>

<script>
import UserAvatar from "./UserAvatar.vue";

export default {
  name: "UserProfile",
  components: {
    UserAvatar,
  },
  props: {
    user: {
      type: Object,
      required: true,
    },
  },
};
</script>
