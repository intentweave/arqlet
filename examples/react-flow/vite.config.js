import {defineConfig} from 'vite';
export default defineConfig({server:{fs:{allow:['../..']},proxy:{
 '/generated':{target:'http://127.0.0.1:8765',changeOrigin:true},
 '/api':{target:'http://127.0.0.1:8765',changeOrigin:true,headers:{Origin:'http://127.0.0.1:8765'}}
}}});
