// API utilities
export const getUser = async (id) => {
  const response = await fetch(`/api/users/${id}`)
  if (!response.ok) {
    throw new Error(`Failed to fetch user ${id}`)
  }
  return response.json()
}

export const updateUser = async (id, userData) => {
  const response = await fetch(`/api/users/${id}`, {
    method: 'PUT',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(userData),
  })
  if (!response.ok) {
    throw new Error(`Failed to update user ${id}`)
  }
  return response.json()
}
