import{b as aa,a as V,f as I,s as ta}from"../chunks/tNClD3uR.js";import{F as j,h as w,e as F,T as C,a as Ma,o as Ga,U as Xa,j as ra,V as ja,W as oa,X as qa,Y as Ja,Z as Wa,_ as Ya,$ as Ua,N as Za,b as Ka,O as Qa,a0 as ae,i as Ea,a1 as ee,p as se,z as te,B as e,C as a,x as Sa,y as s,l as re,a2 as ea,g as Ta,a3 as oe}from"../chunks/C1ORylEu.js";import{b as Aa,d as sa,e as ne,B as Oa,a as ie,c as le,s as y,f as ce}from"../chunks/DLaVBOsx.js";import{b as Ra}from"../chunks/5rjlGHyy.js";import{p as La}from"../chunks/A5L4_TUm.js";function l(f,u,g=!1,h=!1,v=!1,c=!1){var o=f,d="";if(g){var b=f;w&&(o=F(C(b)))}j(()=>{var m=Ga;if(d===(d=u()??"")){w&&Ma();return}if(g&&!w){m.nodes=null,b.innerHTML=d,d!==""&&aa(C(b),b.lastChild);return}if(m.nodes!==null&&(Xa(m.nodes.start,m.nodes.end),m.nodes=null),d!==""){if(w){ra.data;for(var i=Ma(),_=i;i!==null&&(i.nodeType!==ja||i.data!=="");)_=i,i=oa(i);if(i===null)throw qa(),Ja;aa(ra,_),o=F(i);return}var n=h?Ya:v?Ua:void 0,t=Wa(h?"svg":v?"math":"template",n);t.innerHTML=d;var r=h||v?t:t.content;if(aa(C(r),r.lastChild),h||v)for(;C(r);)o.before(C(r));else o.before(r)}})}function de(f,u){let g=null,h=w;var v;if(w){g=ra;for(var c=C(document.head);c!==null&&(c.nodeType!==ja||c.data!==f);)c=oa(c);if(c===null)Ea(!1);else{var o=oa(c);c.remove(),F(o)}}w||(v=document.head.appendChild(Za()));try{Ka(()=>{var d=Qa(()=>u(v));d.f|=ae})}finally{h&&(Ea(!0),F(g))}}ee();/**
 * @license lucide-static v1.33.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */const R=`
<svg
  class="lucide lucide-check"
  xmlns="http://www.w3.org/2000/svg"
  width="24"
  height="24"
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
>
  <path d="M20 6 9 17l-5-5" />
</svg>
`;/**
 * @license lucide-static v1.33.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */const L=`
<svg
  class="lucide lucide-copy"
  xmlns="http://www.w3.org/2000/svg"
  width="24"
  height="24"
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
>
  <rect width="14" height="14" x="8" y="8" rx="2" ry="2" />
  <path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" />
</svg>
`;var he=I('<pre class="disasm svelte-1wng8hg" aria-hidden="true"> </pre>');function ve(f,u){se(u,!0);let g=La(u,"rows",3,20),h=La(u,"seed",3,"berm");function v(n){let t=2166136261;for(let r=0;r<n.length;r++)t^=n.charCodeAt(r),t=Math.imul(t,16777619);return t>>>0}function c(n){let t=n;return()=>{t|=0,t=t+1831565813|0;let r=Math.imul(t^t>>>15,1|t);return r=r+Math.imul(r^r>>>7,61|r)^r,((r^r>>>14)>>>0)/4294967296}}const o=["ra","sp","t0","t1","s0","s1","a0","a1","a2","a3","a4","a5"],d=[["addi","rri"],["addiw","rri"],["slli","rri"],["andi","rri"],["add","rrr"],["sub","rrr"],["or","rrr"],["ld","mem"],["sd","mem"],["lw","mem"],["sw","mem"],["mv","rr"],["sext.w","rr"],["lui","ri"],["auipc","ri"],["beq","br"],["bne","br"],["jal","j"],["j","j"],["ret","none"]],b=65700,m=(()=>{const n=c(v(h())),t=p=>p[Math.floor(n()*p.length)],r=(p,k)=>p.toString(16).padStart(k,"0"),M=()=>`0x${r(b+Math.floor(n()*400)*2,5)}`,D=p=>{switch(p){case"rri":return`${t(o)},${t(o)},${Math.floor(n()*128)-64}`;case"rrr":return`${t(o)},${t(o)},${t(o)}`;case"mem":return`${t(o)},${Math.floor(n()*32)*8}(sp)`;case"rr":return`${t(o)},${t(o)}`;case"ri":return`${t(o)},0x${r(Math.floor(n()*4096),3)}`;case"br":return`${t(o)},${t(o)},${M()}`;case"j":return M();default:return""}};let E=b;return Array.from({length:g()},()=>{const[p,k]=t(d),S=n()<.55,T=r(Math.floor(n()*(S?65536:4294967296)),S?4:8),N=`${r(E,8)}  ${T.padEnd(10)}${p.padEnd(8)}${D(k)}`;return E+=S?2:4,N}).join(`
`)})();var i=he(),_=e(i,!0);a(i),j(()=>ta(_,m)),V(f,i),te()}var pe=I('<meta name="description"/> <meta property="og:title"/> <meta property="og:description"/> <!>',1),ue=I('<article class="svelte-1uha8ag"><h2 class="svelte-1uha8ag"> </h2> <p class="svelte-1uha8ag"> </p></article>'),ge=I(`<section class="hero svelte-1uha8ag"><div class="say"><h1 class="svelte-1uha8ag">Nothing survives the call.</h1> <p class="lede svelte-1uha8ag">The OS for agent harnesses. One statically linked RV64 ELF, pinned by hash, compiled once,
			and instantiated per invocation.</p> <div class="install code-block svelte-1uha8ag"><span class="file svelte-1uha8ag">crates.io</span> <code class="svelte-1uha8ag">cargo add berm</code> <button class="copy" type="button" aria-label="Copy"><!><!></button></div> <p class="facts svelte-1uha8ag"><span><strong class="svelte-1uha8ag">Apache-2.0</strong> licensed</span> <span>Compiled by <strong class="svelte-1uha8ag">Cranelift</strong></span> <span>Served over <strong class="svelte-1uha8ag">MCP</strong></span></p></div> <div class="act svelte-1uha8ag"><!> <div class="cta svelte-1uha8ag"><a class="button primary svelte-1uha8ag">Read the docs</a></div></div></section> <section class="call svelte-1uha8ag"><div class="prose svelte-1uha8ag"><h2 class="svelte-1uha8ag">Two results, and they mean different things.</h2> <p class="svelte-1uha8ag">The outer result is the host's — a missing tool, a trap. The inner one is the program
			reporting failure, which is a result the model should see.</p></div> <div class="code-block"><pre><code></code></pre> <button class="copy" type="button" aria-label="Copy code"><!><!></button></div></section> <section class="stage svelte-1uha8ag"><div class="window svelte-1uha8ag"><div class="titlebar svelte-1uha8ag"><span class="light close svelte-1uha8ag" aria-hidden="true"></span> <span class="light minimise svelte-1uha8ag" aria-hidden="true"></span> <span class="light zoom svelte-1uha8ag" aria-hidden="true"></span> <span class="title svelte-1uha8ag">berm push</span></div> <div class="code-block"><pre><code></code></pre> <button class="copy" type="button" aria-label="Copy transcript"><!><!></button></div></div> <p class="note svelte-1uha8ag">A program travels as one OCI layer with no tarball around it, so the digest a registry
		addresses it by is <code>shasum</code> of the file on your disk.</p></section> <section class="pillars svelte-1uha8ag"><div class="grid svelte-1uha8ag"></div></section> <section class="cli svelte-1uha8ag"><div class="column svelte-1uha8ag"><h2 class="svelte-1uha8ag">Running</h2> <p class="svelte-1uha8ag"><code>bermd</code> serves every deployed program on one MCP endpoint, with tools named <code></code>.</p> <div class="code-block"><pre><code></code></pre> <button class="copy" type="button" aria-label="Copy commands"><!><!></button></div></div> <div class="column svelte-1uha8ag"><h2 class="svelte-1uha8ag">Moving</h2> <p class="svelte-1uha8ag">Pushing makes a program fetchable, not findable. The list is a git repository, so <code>search</code> reads a clone of it with no service and no credential.</p> <div class="code-block"><pre><code></code></pre> <button class="copy" type="button" aria-label="Copy commands"><!><!></button></div></div></section> <footer class="svelte-1uha8ag"><nav class="left svelte-1uha8ag"><a href="https://github.com/crabtalk" class="svelte-1uha8ag">crabtalk</a> <a class="svelte-1uha8ag">Docs</a></nav> <nav class="right svelte-1uha8ag"><a aria-label="berm on GitHub" class="svelte-1uha8ag"><!></a> <a target="_blank" rel="noreferrer" aria-label="The author on X" class="svelte-1uha8ag"><!></a></nav></footer>`,1);function we(f){const u="berm — the OS for agent harnesses",g=`<span class="mark">let</span> berm = Berm::load(&amp;engine, &amp;elf, &amp;[])?;

<span class="mark">match</span> berm.call(<span class="s">"echo"</span>, <span class="s">br#"{"query":"hello"}"#</span>.to_vec())? {
    Ok(result) =&gt; println!(<span class="s">"{result}"</span>),
    Err(failure) =&gt; eprintln!(<span class="s">"{failure}"</span>),
}`,h="222890c498ed28f4bf60670a223141489d9879020bd1890111b8c11ac79fa31d",v=`<span class="c">$</span> berm push 127.0.0.1:5000/berm/fixture:v1 ./fixture
127.0.0.1:5000/berm/fixture:v1
  digest  <span class="mark">sha256:${h}</span>

<span class="c">$</span> shasum -a256 ./fixture
<span class="mark">${h}</span>  ./fixture`,c=`<span class="c">$</span> bermd &amp;
<span class="c">$</span> berm deploy example ./program.elf
<span class="c">$</span> berm ls`,o=`<span class="c">$</span> berm push ghcr.io/org/example:v1 ./program.elf
<span class="c">$</span> berm publish ghcr.io/org/example:v1
<span class="c">$</span> berm search <span class="s">"read a file"</span>`,d=[{title:"Pinned by hash",body:"A program is one statically linked RV64 ELF. berm compiles it once and instantiates it per invocation — arguments go in through syscalls, the result comes back out of guest memory, and the instance is gone."},{title:"The linker is the boundary",body:"A program reaches the world only through the syscalls it was given, and that list is the Linker it was instantiated with. A call to anything else traps because nothing is registered for it, not because a check said no."},{title:"berm has no host",body:"berm ships no syscalls at all. What a filesystem is bounded by, and where bytes persist, are decisions about a host — so they belong to whoever is building one."},{title:"Read it without running it",body:"Manifest::from_elf reads what an image claims to be — its tools, their schemas, when to reach for them — without compiling it and without running it."}],m=`<script type="application/ld+json">${JSON.stringify({"@context":"https://schema.org","@type":"SoftwareSourceCode",name:"berm",description:sa,codeRepository:Aa,programmingLanguage:"Rust",license:"https://www.apache.org/licenses/LICENSE-2.0",keywords:["RISC-V sandbox","RV64 ELF","Cranelift JIT","MCP server","agent tools","LLM tool sandbox","OCI artifact","Rust sandbox"]})}<\/script>`;var i=ge();de("1uha8ag",A=>{var O=pe(),$=Sa(O),x=s($,2);y(x,"content",u);var B=s(x,2),H=s(B,2);l(H,()=>m),j(()=>{y($,"content",sa),y(B,"content",sa)}),re(()=>{oe.title="berm — the OS for agent harnesses"}),V(A,O)});var _=Sa(i),n=e(_),t=s(e(n),4),r=s(e(t),4),M=e(r);l(M,()=>L);var D=s(M);l(D,()=>R),a(r),a(t),ea(2),a(n);var E=s(n,2),p=e(E);ve(p,{});var k=s(p,2),S=e(k);a(k),a(E),a(_);var T=s(_,2),N=s(e(T),2),z=e(N),na=e(z);l(na,()=>g,!0),a(na),a(z);var ia=s(z,2),la=e(ia);l(la,()=>L);var Na=s(la);l(Na,()=>R),a(ia),a(N),a(T);var G=s(T,2),ca=e(G),da=s(e(ca),2),X=e(da),ha=e(X);l(ha,()=>v,!0),a(ha),a(X);var va=s(X,2),pa=e(va);l(pa,()=>L);var Pa=s(pa);l(Pa,()=>R),a(va),a(da),a(ca),ea(2),a(G);var q=s(G,2),ua=e(q);ne(ua,5,()=>d,A=>A.title,(A,O)=>{var $=ue(),x=e($),B=e(x,!0);a(x);var H=s(x,2),za=e(H,!0);a(H),a($),j(()=>{ta(B,Ta(O).title),ta(za,Ta(O).body)}),V(A,$)}),a(ua),a(q);var J=s(q,2),W=e(J),Y=s(e(W),2),Ba=s(e(Y),2);Ba.textContent="{program}.{tool}",ea(),a(Y);var ga=s(Y,2),U=e(ga),ma=e(U);l(ma,()=>c,!0),a(ma),a(U);var fa=s(U,2),ba=e(fa);l(ba,()=>L);var Ha=s(ba);l(Ha,()=>R),a(fa),a(ga),a(W);var _a=s(W,2),ya=s(e(_a),4),Z=e(ya),wa=e(Z);l(wa,()=>o,!0),a(wa),a(Z);var ka=s(Z,2),$a=e(ka);l($a,()=>L);var Va=s($a);l(Va,()=>R),a(ka),a(ya),a(_a),a(J);var xa=s(J,2),K=e(xa),Fa=s(e(K),2);a(K);var Ca=s(K,2),P=e(Ca),Ia=e(P);Oa(Ia,{get icon(){return ie},size:15}),a(P);var Q=s(P,2),Da=e(Q);Oa(Da,{get icon(){return le},size:14}),a(Q),a(Ca),a(xa),j(()=>{y(S,"href",`${Ra??""}/book/`),y(Fa,"href",`${Ra??""}/book/`),y(P,"href",Aa),y(Q,"href",ce)}),V(f,i)}export{we as component};
