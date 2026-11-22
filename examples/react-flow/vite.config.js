import {defineConfig} from 'vite';
// The loopback backend of the playground; ARQLET_BACKEND points the proxy at another port.
const backend=process.env.ARQLET_BACKEND??'http://127.0.0.1:8765';
export default defineConfig({server:{fs:{allow:['../..']},proxy:{
 '/generated':{target:backend,changeOrigin:true},
 '/api':{target:backend,changeOrigin:true,headers:{Origin:backend}}
}}});
