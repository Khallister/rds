import { helper2 } from './helper2.js';

export function helper1() {
    console.log('Helper 1');
    helper2(); // This creates a circular dependency
}
