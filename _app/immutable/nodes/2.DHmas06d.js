import{b as ee,a as H,f as V,s as re}from"../chunks/tNClD3uR.js";import{F as O,h as w,e as I,T as M,a as Ae,o as Xe,U as qe,j as ne,V as Ne,W as oe,X as Je,Y as We,Z as Ye,_ as Ue,$ as Ze,N as Ke,b as Qe,O as ea,a0 as aa,i as Te,a1 as sa,p as ta,z as ra,B as a,C as e,x as Re,y as s,l as na,a2 as ae,g as Le,a3 as oa}from"../chunks/C1ORylEu.js";import{b as Se,d as se,e as ia,B as je,a as la,c as ca,s as y,f as da}from"../chunks/B2302l4H.js";import{b as te}from"../chunks/BlaC8iix.js";import{p as Oe}from"../chunks/A5L4_TUm.js";function l(b,p,g=!1,h=!1,v=!1,c=!1){var n=b,d="";if(g){var f=b;w&&(n=I(M(f)))}O(()=>{var m=Xe;if(d===(d=p()??"")){w&&Ae();return}if(g&&!w){m.nodes=null,f.innerHTML=d,d!==""&&ee(M(f),f.lastChild);return}if(m.nodes!==null&&(qe(m.nodes.start,m.nodes.end),m.nodes=null),d!==""){if(w){ne.data;for(var i=Ae(),_=i;i!==null&&(i.nodeType!==Ne||i.data!=="");)_=i,i=oe(i);if(i===null)throw Je(),We;ee(ne,_),n=I(i);return}var o=h?Ue:v?Ze:void 0,t=Ye(h?"svg":v?"math":"template",o);t.innerHTML=d;var r=h||v?t:t.content;if(ee(M(r),r.lastChild),h||v)for(;M(r);)n.before(M(r));else n.before(r)}})}function ha(b,p){let g=null,h=w;var v;if(w){g=ne;for(var c=M(document.head);c!==null&&(c.nodeType!==Ne||c.data!==b);)c=oe(c);if(c===null)Te(!1);else{var n=oe(c);c.remove(),I(n)}}w||(v=document.head.appendChild(Ke()));try{Qe(()=>{var d=ea(()=>p(v));d.f|=aa})}finally{h&&(Te(!0),I(g))}}sa();/**
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
`;var va=V('<pre class="disasm svelte-1wng8hg" aria-hidden="true"> </pre>');function ua(b,p){ta(p,!0);let g=Oe(p,"rows",3,20),h=Oe(p,"seed",3,"berm");function v(o){let t=2166136261;for(let r=0;r<o.length;r++)t^=o.charCodeAt(r),t=Math.imul(t,16777619);return t>>>0}function c(o){let t=o;return()=>{t|=0,t=t+1831565813|0;let r=Math.imul(t^t>>>15,1|t);return r=r+Math.imul(r^r>>>7,61|r)^r,((r^r>>>14)>>>0)/4294967296}}const n=["ra","sp","t0","t1","s0","s1","a0","a1","a2","a3","a4","a5"],d=[["addi","rri"],["addiw","rri"],["slli","rri"],["andi","rri"],["add","rrr"],["sub","rrr"],["or","rrr"],["ld","mem"],["sd","mem"],["lw","mem"],["sw","mem"],["mv","rr"],["sext.w","rr"],["lui","ri"],["auipc","ri"],["beq","br"],["bne","br"],["jal","j"],["j","j"],["ret","none"]],f=65700,m=(()=>{const o=c(v(h())),t=u=>u[Math.floor(o()*u.length)],r=(u,k)=>u.toString(16).padStart(k,"0"),E=()=>`0x${r(f+Math.floor(o()*400)*2,5)}`,F=u=>{switch(u){case"rri":return`${t(n)},${t(n)},${Math.floor(o()*128)-64}`;case"rrr":return`${t(n)},${t(n)},${t(n)}`;case"mem":return`${t(n)},${Math.floor(o()*32)*8}(sp)`;case"rr":return`${t(n)},${t(n)}`;case"ri":return`${t(n)},0x${r(Math.floor(o()*4096),3)}`;case"br":return`${t(n)},${t(n)},${E()}`;case"j":return E();default:return""}};let A=f;return Array.from({length:g()},()=>{const[u,k]=t(d),x=o()<.55,D=r(Math.floor(o()*(x?65536:4294967296)),x?4:8),T=`${r(A,8)}  ${D.padEnd(10)}${u.padEnd(8)}${F(k)}`;return A+=x?2:4,T}).join(`
`)})();var i=va(),_=a(i,!0);e(i),O(()=>re(_,m)),H(b,i),ra()}var pa=V('<meta name="description"/> <meta property="og:title"/> <meta property="og:description"/> <!>',1),ga=V('<article class="svelte-1uha8ag"><h2 class="svelte-1uha8ag"> </h2> <p class="svelte-1uha8ag"> </p></article>'),ma=V(`<section class="hero svelte-1uha8ag"><div class="say"><h1 class="svelte-1uha8ag">Nothing survives the call.</h1> <p class="lede svelte-1uha8ag">A sandbox for harnesses. One statically linked RV64 ELF, pinned by hash, compiled once,
			and instantiated per invocation.</p> <div class="install code-block svelte-1uha8ag"><span class="file svelte-1uha8ag">crates.io</span> <code class="svelte-1uha8ag">cargo add berm</code> <button class="copy" type="button" aria-label="Copy"><!><!></button></div> <p class="facts svelte-1uha8ag"><span><strong class="svelte-1uha8ag">Apache-2.0</strong> licensed</span> <span>Compiled by <strong class="svelte-1uha8ag">Cranelift</strong></span> <span>Served over <strong class="svelte-1uha8ag">MCP</strong></span></p></div> <div class="act svelte-1uha8ag"><!> <div class="cta svelte-1uha8ag"><a class="button primary svelte-1uha8ag">Read the docs</a> <a class="button svelte-1uha8ag">API reference</a></div></div></section> <section class="call svelte-1uha8ag"><div class="prose svelte-1uha8ag"><h2 class="svelte-1uha8ag">Two results, and they mean different things.</h2> <p class="svelte-1uha8ag">The outer result is the host's — a missing tool, a trap. The inner one is the harness
			reporting failure, which is a result the model should see.</p></div> <div class="code-block"><pre><code></code></pre> <button class="copy" type="button" aria-label="Copy code"><!><!></button></div></section> <section class="stage svelte-1uha8ag"><div class="window svelte-1uha8ag"><div class="titlebar svelte-1uha8ag"><span class="light close svelte-1uha8ag" aria-hidden="true"></span> <span class="light minimise svelte-1uha8ag" aria-hidden="true"></span> <span class="light zoom svelte-1uha8ag" aria-hidden="true"></span> <span class="title svelte-1uha8ag">berm push</span></div> <div class="code-block"><pre><code></code></pre> <button class="copy" type="button" aria-label="Copy transcript"><!><!></button></div></div> <p class="note svelte-1uha8ag">A harness travels as one OCI layer with no tarball around it, so the digest a registry
		addresses it by is <code>shasum</code> of the file on your disk.</p></section> <section class="pillars svelte-1uha8ag"><div class="grid svelte-1uha8ag"></div></section> <section class="cli svelte-1uha8ag"><div class="column svelte-1uha8ag"><h2 class="svelte-1uha8ag">Running</h2> <p class="svelte-1uha8ag"><code>bermd</code> serves every deployed harness on one MCP endpoint, with tools named <code></code>.</p> <div class="code-block"><pre><code></code></pre> <button class="copy" type="button" aria-label="Copy commands"><!><!></button></div></div> <div class="column svelte-1uha8ag"><h2 class="svelte-1uha8ag">Moving</h2> <p class="svelte-1uha8ag">Pushing makes a harness fetchable, not findable. The list is a git repository, so <code>search</code> reads a clone of it with no service and no credential.</p> <div class="code-block"><pre><code></code></pre> <button class="copy" type="button" aria-label="Copy commands"><!><!></button></div></div></section> <footer class="svelte-1uha8ag"><nav class="left svelte-1uha8ag"><a href="https://github.com/crabtalk" class="svelte-1uha8ag">crabtalk</a> <a class="svelte-1uha8ag">Docs</a></nav> <nav class="right svelte-1uha8ag"><a aria-label="berm on GitHub" class="svelte-1uha8ag"><!></a> <a target="_blank" rel="noreferrer" aria-label="The author on X" class="svelte-1uha8ag"><!></a></nav></footer>`,1);function ka(b){const p="berm — a sandbox for harnesses",g=`<span class="mark">let</span> berm = Berm::load(&amp;engine, &amp;elf, &amp;[])?;

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
<span class="c">$</span> berm search <span class="s">"read a file"</span>`,d=[{title:"Pinned by hash",body:"A harness is one statically linked RV64 ELF. berm compiles it once and instantiates it per invocation — arguments go in through host calls, the result comes back out of guest memory, and the instance is gone."},{title:"The linker is the boundary",body:"A harness reaches the world only through the system harnesses it was given, and that list is the Linker it was instantiated with. A call to anything else traps because nothing is registered for it, not because a check said no."},{title:"berm has no host",body:"berm ships no system harnesses at all. What a filesystem is bounded by, and where bytes persist, are decisions about a host — so they belong to whoever is building one."},{title:"Read it without running it",body:"Manifest::from_elf reads what an image claims to be — its tools, their schemas, when to reach for them — without compiling it and without running it."}],m=`<script type="application/ld+json">${JSON.stringify({"@context":"https://schema.org","@type":"SoftwareSourceCode",name:"berm",description:se,codeRepository:Se,programmingLanguage:"Rust",license:"https://www.apache.org/licenses/LICENSE-2.0",keywords:["RISC-V sandbox","RV64 ELF","Cranelift JIT","MCP server","agent tools","LLM tool sandbox","OCI artifact","Rust sandbox"]})}<\/script>`;var i=ma();ha("1uha8ag",R=>{var L=pa(),$=Re(L),C=s($,2);y(C,"content",p);var P=s(C,2),B=s(P,2);l(B,()=>m),O(()=>{y($,"content",se),y(P,"content",se)}),na(()=>{oa.title="berm — a sandbox for harnesses"}),H(R,L)});var _=Re(i),o=a(_),t=s(a(o),4),r=s(a(t),4),E=a(r);l(E,()=>j);var F=s(E);l(F,()=>S),e(r),e(t),ae(2),e(o);var A=s(o,2),u=a(A);ua(u,{});var k=s(u,2),x=a(k),D=s(x,2);e(k),e(A),e(_);var T=s(_,2),ie=s(a(T),2),z=a(ie),le=a(z);l(le,()=>g,!0),e(le),e(z);var ce=s(z,2),de=a(ce);l(de,()=>j);var Pe=s(de);l(Pe,()=>S),e(ce),e(ie),e(T);var G=s(T,2),he=a(G),ve=s(a(he),2),X=a(ve),ue=a(X);l(ue,()=>v,!0),e(ue),e(X);var pe=s(X,2),ge=a(pe);l(ge,()=>j);var Be=s(ge);l(Be,()=>S),e(pe),e(ve),e(he),ae(2),e(G);var q=s(G,2),me=a(q);ia(me,5,()=>d,R=>R.title,(R,L)=>{var $=ga(),C=a($),P=a(C,!0);e(C);var B=s(C,2),Ge=a(B,!0);e(B),e($),O(()=>{re(P,Le(L).title),re(Ge,Le(L).body)}),H(R,$)}),e(me),e(q);var J=s(q,2),W=a(J),Y=s(a(W),2),He=s(a(Y),2);He.textContent="{harness}.{tool}",ae(),e(Y);var be=s(Y,2),U=a(be),fe=a(U);l(fe,()=>c,!0),e(fe),e(U);var _e=s(U,2),ye=a(_e);l(ye,()=>j);var Ie=s(ye);l(Ie,()=>S),e(_e),e(be),e(W);var we=s(W,2),ke=s(a(we),4),Z=a(ke),xe=a(Z);l(xe,()=>n,!0),e(xe),e(Z);var $e=s(Z,2),Ce=a($e);l(Ce,()=>j);var Ve=s(Ce);l(Ve,()=>S),e($e),e(ke),e(we),e(J);var Me=s(J,2),K=a(Me),Fe=s(a(K),2);e(K);var Ee=s(K,2),N=a(Ee),De=a(N);je(De,{get icon(){return la},size:15}),e(N);var Q=s(N,2),ze=a(Q);je(ze,{get icon(){return ca},size:14}),e(Q),e(Ee),e(Me),O(()=>{y(x,"href",`${te??""}/book/`),y(D,"href",`${te??""}/api/`),y(Fe,"href",`${te??""}/book/`),y(N,"href",Se),y(Q,"href",da)}),H(b,i)}export{ka as component};
