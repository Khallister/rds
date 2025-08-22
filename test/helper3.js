import { helper1 } from './helper1.js';

export function helper2() {
    console.log('Helper 2');
    // This creates a circular dependency with helper1
}
