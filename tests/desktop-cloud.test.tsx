// @vitest-environment jsdom
import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { CloudPanel } from "../desktop-app/src/CloudPanel";
import { call } from "../desktop-app/src/bridge";
vi.mock("../desktop-app/src/bridge",()=>({call:vi.fn(),message:(e:unknown)=>String(e)}));
const invoke=vi.mocked(call);
let state: Record<string,unknown>;
beforeEach(()=>{state={connected:false,email:null,connectUrl:null,code:null,error:null,jobs:[]};invoke.mockReset();invoke.mockImplementation(async()=>state);});
afterEach(cleanup);
describe("desktop Drive panel",()=>{
 it("starts device authorization and shows its confirmation code",async()=>{
  invoke.mockImplementation(async(command)=>{if(command==="cloud_connect")state={...state,connectUrl:"https://tinrec.vercel.app/connect?code=test",code:"test"};return state;});
  render(<CloudPanel files={[]}/>);
  await screen.findByText("Conectar Google");fireEvent.click(screen.getByText("Conectar Google"));
  await screen.findByText("Aprove a conexão no navegador");expect(screen.getByDisplayValue("https://tinrec.vercel.app/connect?code=test")).toBeTruthy();expect(invoke).toHaveBeenCalledWith("cloud_connect",undefined);
 });
 it("shows early link and retry without exposing credentials",async()=>{
  state={...state,connected:true,email:"user@example.test",jobs:[{id:"one",fileName:"one.mp4",title:"Minha gravação",status:"error",uploadedBytes:30,sizeBytes:100,url:"https://tinrec.vercel.app/v/one",error:"Sem conexão"}]};
  render(<CloudPanel files={[]}/>);await screen.findByText("Minha gravação");
  expect(screen.getByDisplayValue("https://tinrec.vercel.app/v/one")).toBeTruthy();
  fireEvent.click(screen.getByText("Copiar link"));await waitFor(()=>expect(invoke).toHaveBeenCalledWith("cloud_copy",{id:"one"}));
  fireEvent.click(screen.getByText("Tentar novamente"));await waitFor(()=>expect(invoke).toHaveBeenCalledWith("cloud_retry",{id:"one"}));
 });
 it("offers only completed local recordings for explicit upload",async()=>{
  state={...state,connected:true,email:"user@example.test"};
  render(<CloudPanel files={[{fileName:"ok.mp4",title:"Pronto",bytes:50,modifiedMs:0,duration:1,status:"completed"},{fileName:"bad.mp4",title:"Falhou",bytes:50,modifiedMs:0,duration:1,status:"failed"}]}/>);
  await screen.findByText("Enviar ao Drive");expect(screen.queryByText("Falhou")).toBeNull();
  fireEvent.click(screen.getByText("Enviar ao Drive"));await waitFor(()=>expect(invoke).toHaveBeenCalledWith("cloud_enqueue",{fileName:"ok.mp4"}));
 });
});
