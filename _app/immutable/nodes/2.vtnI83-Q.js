import{b as ee,a as V,f as I,s as te}from"../chunks/tNClD3uR.js";import{F as O,h as w,e as F,T as C,a as Me,o as Ge,U as Xe,j as re,V as Oe,W as ne,X as qe,Y as Je,Z as We,_ as Ye,$ as Ue,N as Ze,b as Ke,O as Qe,a0 as ea,i as Ee,a1 as aa,p as sa,z as ta,B as a,C as e,x as Ae,y as s,l as ra,a2 as ae,g as Te,a3 as na}from"../chunks/C1ORylEu.js";import{b as Re,d as se,e as oa,B as Le,a as ia,c as la,s as y,f as ca}from"../chunks/B2302l4H.js";import{b as Se}from"../chunks/CuruW85N.js";import{p as je}from"../chunks/A5L4_TUm.js";function l(b,p,g=!1,h=!1,v=!1,c=!1){var n=b,d="";if(g){var f=b;w&&(n=F(C(f)))}O(()=>{var m=Ge;if(d===(d=p()??"")){w&&Me();return}if(g&&!w){m.nodes=null,f.innerHTML=d,d!==""&&ee(C(f),f.lastChild);return}if(m.nodes!==null&&(Xe(m.nodes.start,m.nodes.end),m.nodes=null),d!==""){if(w){re.data;for(var i=Me(),_=i;i!==null&&(i.nodeType!==Oe||i.data!=="");)_=i,i=ne(i);if(i===null)throw qe(),Je;ee(re,_),n=F(i);return}var o=h?Ye:v?Ue:void 0,t=We(h?"svg":v?"math":"template",o);t.innerHTML=d;var r=h||v?t:t.content;if(ee(C(r),r.lastChild),h||v)for(;C(r);)n.before(C(r));else n.before(r)}})}function da(b,p){let g=null,h=w;var v;if(w){g=re;for(var c=C(document.head);c!==null&&(c.nodeType!==Oe||c.data!==b);)c=ne(c);if(c===null)Ee(!1);else{var n=ne(c);c.remove(),F(n)}}w||(v=document.head.appendChild(Ze()));try{Ke(()=>{var d=Qe(()=>p(v));d.f|=ea})}finally{h&&(Ee(!0),F(g))}}aa();/**
 * @license lucide-static v1.33.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */const S=`
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
 */const j=`
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
`;var ha=I('<pre class="disasm svelte-1wng8hg" aria-hidden="true"> </pre>');function va(b,p){sa(p,!0);let g=je(p,"rows",3,20),h=je(p,"seed",3,"berm");function v(o){let t=2166136261;for(let r=0;r<o.length;r++)t^=o.charCodeAt(r),t=Math.imul(t,16777619);return t>>>0}function c(o){let t=o;return()=>{t|=0,t=t+1831565813|0;let r=Math.imul(t^t>>>15,1|t);return r=r+Math.imul(r^r>>>7,61|r)^r,((r^r>>>14)>>>0)/4294967296}}const n=["ra","sp","t0","t1","s0","s1","a0","a1","a2","a3","a4","a5"],d=[["addi","rri"],["addiw","rri"],["slli","rri"],["andi","rri"],["add","rrr"],["sub","rrr"],["or","rrr"],["ld","mem"],["sd","mem"],["lw","mem"],["sw","mem"],["mv","rr"],["sext.w","rr"],["lui","ri"],["auipc","ri"],["beq","br"],["bne","br"],["jal","j"],["j","j"],["ret","none"]],f=65700,m=(()=>{const o=c(v(h())),t=u=>u[Math.floor(o()*u.length)],r=(u,k)=>u.toString(16).padStart(k,"0"),M=()=>`0x${r(f+Math.floor(o()*400)*2,5)}`,D=u=>{switch(u){case"rri":return`${t(n)},${t(n)},${Math.floor(o()*128)-64}`;case"rrr":return`${t(n)},${t(n)},${t(n)}`;case"mem":return`${t(n)},${Math.floor(o()*32)*8}(sp)`;case"rr":return`${t(n)},${t(n)}`;case"ri":return`${t(n)},0x${r(Math.floor(o()*4096),3)}`;case"br":return`${t(n)},${t(n)},${M()}`;case"j":return M();default:return""}};let E=f;return Array.from({length:g()},()=>{const[u,k]=t(d),A=o()<.55,T=r(Math.floor(o()*(A?65536:4294967296)),A?4:8),N=`${r(E,8)}  ${T.padEnd(10)}${u.padEnd(8)}${D(k)}`;return E+=A?2:4,N}).join(`
`)})();var i=ha(),_=a(i,!0);e(i),O(()=>te(_,m)),V(b,i),ta()}var ua=I('<meta name="description"/> <meta property="og:title"/> <meta property="og:description"/> <!>',1),pa=I('<article class="svelte-1uha8ag"><h2 class="svelte-1uha8ag"> </h2> <p class="svelte-1uha8ag"> </p></article>'),ga=I(`<section class="hero svelte-1uha8ag"><div class="say"><h1 class="svelte-1uha8ag">Nothing survives the call.</h1> <p class="lede svelte-1uha8ag">A sandbox for harnesses. One statically linked RV64 ELF, pinned by hash, compiled once,
			and instantiated per invocation.</p> <div class="install code-block svelte-1uha8ag"><span class="file svelte-1uha8ag">crates.io</span> <code class="svelte-1uha8ag">cargo add berm</code> <button class="copy" type="button" aria-label="Copy"><!><!></button></div> <p class="facts svelte-1uha8ag"><span><strong class="svelte-1uha8ag">Apache-2.0</strong> licensed</span> <span>Compiled by <strong class="svelte-1uha8ag">Cranelift</strong></span> <span>Served over <strong class="svelte-1uha8ag">MCP</strong></span></p></div> <div class="act svelte-1uha8ag"><!> <div class="cta svelte-1uha8ag"><a class="button primary svelte-1uha8ag">Read the docs</a></div></div></section> <section class="call svelte-1uha8ag"><div class="prose svelte-1uha8ag"><h2 class="svelte-1uha8ag">Two results, and they mean different things.</h2> <p class="svelte-1uha8ag">The outer result is the host's — a missing tool, a trap. The inner one is the harness
			reporting failure, which is a result the model should see.</p></div> <div class="code-block"><pre><code></code></pre> <button class="copy" type="button" aria-label="Copy code"><!><!></button></div></section> <section class="stage svelte-1uha8ag"><div class="window svelte-1uha8ag"><div class="titlebar svelte-1uha8ag"><span class="light close svelte-1uha8ag" aria-hidden="true"></span> <span class="light minimise svelte-1uha8ag" aria-hidden="true"></span> <span class="light zoom svelte-1uha8ag" aria-hidden="true"></span> <span class="title svelte-1uha8ag">berm push</span></div> <div class="code-block"><pre><code></code></pre> <button class="copy" type="button" aria-label="Copy transcript"><!><!></button></div></div> <p class="note svelte-1uha8ag">A harness travels as one OCI layer with no tarball around it, so the digest a registry
		addresses it by is <code>shasum</code> of the file on your disk.</p></section> <section class="pillars svelte-1uha8ag"><div class="grid svelte-1uha8ag"></div></section> <section class="cli svelte-1uha8ag"><div class="column svelte-1uha8ag"><h2 class="svelte-1uha8ag">Running</h2> <p class="svelte-1uha8ag"><code>bermd</code> serves every deployed harness on one MCP endpoint, with tools named <code></code>.</p> <div class="code-block"><pre><code></code></pre> <button class="copy" type="button" aria-label="Copy commands"><!><!></button></div></div> <div class="column svelte-1uha8ag"><h2 class="svelte-1uha8ag">Moving</h2> <p class="svelte-1uha8ag">Pushing makes a harness fetchable, not findable. The list is a git repository, so <code>search</code> reads a clone of it with no service and no credential.</p> <div class="code-block"><pre><code></code></pre> <button class="copy" type="button" aria-label="Copy commands"><!><!></button></div></div></section> <footer class="svelte-1uha8ag"><nav class="left svelte-1uha8ag"><a href="https://github.com/crabtalk" class="svelte-1uha8ag">crabtalk</a> <a class="svelte-1uha8ag">Docs</a></nav> <nav class="right svelte-1uha8ag"><a aria-label="berm on GitHub" class="svelte-1uha8ag"><!></a> <a target="_blank" rel="noreferrer" aria-label="The author on X" class="svelte-1uha8ag"><!></a></nav></footer>`,1);function wa(b){const p="berm — a sandbox for harnesses",g=`<span class="mark">let</span> berm = Berm::load(&amp;engine, &amp;elf, &amp;[])?;

<span class="mark">match</span> berm.call(<span class="s">"echo"</span>, <span class="s">br#"{"query":"hello"}"#</span>.to_vec())? {
    Ok(result) =&gt; println!(<span class="s">"{result}"</span>),
    Err(failure) =&gt; eprintln!(<span class="s">"{failure}"</span>),
}`,h="222890c498ed28f4bf60670a223141489d9879020bd1890111b8c11ac79fa31d",v=`<span class="c">$</span> berm push 127.0.0.1:5000/berm/fixture:v1 ./fixture
127.0.0.1:5000/berm/fixture:v1
  digest  <span class="mark">sha256:${h}</span>

<span class="c">$</span> shasum -a256 ./fixture
<span class="mark">${h}</span>  ./fixture`,c=`<span class="c">$</span> bermd &amp;
<span class="c">$</span> berm deploy example ./harness.elf
<span class="c">$</span> berm ls`,n=`<span class="c">$</span> berm push ghcr.io/org/example:v1 ./harness.elf
<span class="c">$</span> berm publish ghcr.io/org/example:v1
<span class="c">$</span> berm search <span class="s">"read a file"</span>`,d=[{title:"Pinned by hash",body:"A harness is one statically linked RV64 ELF. berm compiles it once and instantiates it per invocation — arguments go in through host calls, the result comes back out of guest memory, and the instance is gone."},{title:"The linker is the boundary",body:"A harness reaches the world only through the system harnesses it was given, and that list is the Linker it was instantiated with. A call to anything else traps because nothing is registered for it, not because a check said no."},{title:"berm has no host",body:"berm ships no system harnesses at all. What a filesystem is bounded by, and where bytes persist, are decisions about a host — so they belong to whoever is building one."},{title:"Read it without running it",body:"Manifest::from_elf reads what an image claims to be — its tools, their schemas, when to reach for them — without compiling it and without running it."}],m=`<script type="application/ld+json">${JSON.stringify({"@context":"https://schema.org","@type":"SoftwareSourceCode",name:"berm",description:se,codeRepository:Re,programmingLanguage:"Rust",license:"https://www.apache.org/licenses/LICENSE-2.0",keywords:["RISC-V sandbox","RV64 ELF","Cranelift JIT","MCP server","agent tools","LLM tool sandbox","OCI artifact","Rust sandbox"]})}<\/script>`;var i=ga();da("1uha8ag",R=>{var L=ua(),x=Ae(L),$=s(x,2);y($,"content",p);var B=s($,2),H=s(B,2);l(H,()=>m),O(()=>{y(x,"content",se),y(B,"content",se)}),ra(()=>{na.title="berm — a sandbox for harnesses"}),V(R,L)});var _=Ae(i),o=a(_),t=s(a(o),4),r=s(a(t),4),M=a(r);l(M,()=>j);var D=s(M);l(D,()=>S),e(r),e(t),ae(2),e(o);var E=s(o,2),u=a(E);va(u,{});var k=s(u,2),A=a(k);e(k),e(E),e(_);var T=s(_,2),N=s(a(T),2),z=a(N),oe=a(z);l(oe,()=>g,!0),e(oe),e(z);var ie=s(z,2),le=a(ie);l(le,()=>j);var Ne=s(le);l(Ne,()=>S),e(ie),e(N),e(T);var G=s(T,2),ce=a(G),de=s(a(ce),2),X=a(de),he=a(X);l(he,()=>v,!0),e(he),e(X);var ve=s(X,2),ue=a(ve);l(ue,()=>j);var Pe=s(ue);l(Pe,()=>S),e(ve),e(de),e(ce),ae(2),e(G);var q=s(G,2),pe=a(q);oa(pe,5,()=>d,R=>R.title,(R,L)=>{var x=pa(),$=a(x),B=a($,!0);e($);var H=s($,2),ze=a(H,!0);e(H),e(x),O(()=>{te(B,Te(L).title),te(ze,Te(L).body)}),V(R,x)}),e(pe),e(q);var J=s(q,2),W=a(J),Y=s(a(W),2),Be=s(a(Y),2);Be.textContent="{harness}.{tool}",ae(),e(Y);var ge=s(Y,2),U=a(ge),me=a(U);l(me,()=>c,!0),e(me),e(U);var be=s(U,2),fe=a(be);l(fe,()=>j);var He=s(fe);l(He,()=>S),e(be),e(ge),e(W);var _e=s(W,2),ye=s(a(_e),4),Z=a(ye),we=a(Z);l(we,()=>n,!0),e(we),e(Z);var ke=s(Z,2),xe=a(ke);l(xe,()=>j);var Ve=s(xe);l(Ve,()=>S),e(ke),e(ye),e(_e),e(J);var $e=s(J,2),K=a($e),Fe=s(a(K),2);e(K);var Ce=s(K,2),P=a(Ce),Ie=a(P);Le(Ie,{get icon(){return ia},size:15}),e(P);var Q=s(P,2),De=a(Q);Le(De,{get icon(){return la},size:14}),e(Q),e(Ce),e($e),O(()=>{y(A,"href",`${Se??""}/book/`),y(Fe,"href",`${Se??""}/book/`),y(P,"href",Re),y(Q,"href",ca)}),V(b,i)}export{wa as component};
