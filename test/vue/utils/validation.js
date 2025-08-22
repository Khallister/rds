// User validation utilities
export const validateUser = (user) => {
  if (!user || typeof user !== 'object') {
    return false
  }
  
  if (!user.name || typeof user.name !== 'string' || user.name.trim().length === 0) {
    return false
  }
  
  if (!user.email || typeof user.email !== 'string' || !isValidEmail(user.email)) {
    return false
  }
  
  return true
}

export const isValidEmail = (email) => {
  const emailRegex = /^[^\s@]+@[^\s@]+\.[^\s@]+$/
  return emailRegex.test(email)
}

export const validateUserAge = (age) => {
  return typeof age === 'number' && age >= 0 && age <= 150
}
